//! 口令会话只持有内存 SQLite；落盘和恢复快照始终经过认证加密。

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};
use sha2::{Digest, Sha256};

use super::restore_validation::{self, RestoreLimits, SqlBudget};
use super::{copy_database, migrations, sidecar_path, Db};
use crate::error::{AppError, AppResult};
use crate::models::RestoreSummary;
use crate::vault::{self, VaultSession, VaultStatus};

#[derive(Clone, PartialEq, Eq)]
pub(super) struct SourceFingerprint {
    bytes: u64,
    sha256: [u8; 32],
    identity: (u64, u64),
    modified: std::time::SystemTime,
}

#[cfg(windows)]
fn file_identity(file: &File) -> AppResult<(u64, u64)> {
    use std::os::windows::io::AsRawHandle;
    #[repr(C)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    #[repr(C)]
    struct FileInformation {
        attributes: u32,
        creation: FileTime,
        access: FileTime,
        write: FileTime,
        volume: u32,
        size_high: u32,
        size_low: u32,
        links: u32,
        index_high: u32,
        index_low: u32,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetFileInformationByHandle(
            handle: *mut std::ffi::c_void,
            info: *mut FileInformation,
        ) -> i32;
    }
    let mut info = std::mem::MaybeUninit::<FileInformation>::uninit();
    // File 提供有效句柄，Win32 成功时完整初始化固定布局结构。
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), info.as_mut_ptr()) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let info = unsafe { info.assume_init() };
    Ok((
        info.volume as u64,
        ((info.index_high as u64) << 32) | info.index_low as u64,
    ))
}

#[cfg(unix)]
fn file_identity(file: &File) -> AppResult<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata()?;
    Ok((metadata.dev(), metadata.ino()))
}

#[cfg(not(any(windows, unix)))]
fn file_identity(_file: &File) -> AppResult<(u64, u64)> {
    Ok((0, 0))
}

pub(super) fn source_fingerprint(file: &mut File) -> AppResult<SourceFingerprint> {
    let before = file.metadata()?;
    if !before.is_file() || before.len() > vault::MAX_IMAGE_BYTES as u64 {
        return Err(AppError::validation(
            "迁移来源必须是大小不超过 128 MiB 的普通文件",
        ));
    }
    let identity = file_identity(file)?;
    let modified = before.modified()?;
    file.seek(SeekFrom::Start(0))?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut bytes = 0_u64;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        bytes = bytes.saturating_add(read as u64);
        if bytes > vault::MAX_IMAGE_BYTES as u64 {
            return Err(AppError::Conflict(
                "迁移来源在读取中增长，未替换原文件".into(),
            ));
        }
        digest.update(&buffer[..read]);
    }
    let after = file.metadata()?;
    if bytes != before.len()
        || after.len() != before.len()
        || after.modified()? != modified
        || file_identity(file)? != identity
    {
        return Err(AppError::Conflict(
            "迁移来源在读取中变化，未替换原文件".into(),
        ));
    }
    Ok(SourceFingerprint {
        bytes,
        sha256: digest.finalize().into(),
        identity,
        modified,
    })
}

/// Windows 持有 READ|DELETE 共享租约：允许自身原子发布/删除，拒绝其他写句柄。
/// Unix 仅能用指纹检测已发生的变化，不能宣称排除任意不合作写者的最终竞态。
pub(super) struct SourceLease {
    file: File,
    expected: SourceFingerprint,
}

impl SourceLease {
    pub(super) fn acquire(path: &Path) -> AppResult<Self> {
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(5); // FILE_SHARE_READ | FILE_SHARE_DELETE
        }
        let mut file = options.open(path).map_err(|_| {
            AppError::Conflict("来源正在被写入或无法取得防写租约，未替换原文件".into())
        })?;
        let expected = source_fingerprint(&mut file)?;
        let mut lease = Self { file, expected };
        lease.verify_unchanged(path)?;
        Ok(lease)
    }

    pub(super) fn acquire_verified(path: &Path, expected: &SourceFingerprint) -> AppResult<Self> {
        let lease = Self::acquire(path)?;
        if lease.expected != *expected {
            return Err(AppError::Conflict(
                "旧数据库在关闭连接后发生变化，未替换原文件".into(),
            ));
        }
        Ok(lease)
    }

    fn verify_unchanged(&mut self, path: &Path) -> AppResult<()> {
        if source_fingerprint(&mut self.file)? != self.expected
            || source_fingerprint(&mut File::open(path)?)? != self.expected
        {
            return Err(AppError::Conflict(
                "来源内容或文件身份已改变，保留原文件并取消转换".into(),
            ));
        }
        Ok(())
    }
}

fn limits() -> RestoreLimits {
    RestoreLimits {
        max_file_bytes: vault::MAX_IMAGE_BYTES as u64,
        ..Default::default()
    }
}

fn memory_connection() -> AppResult<Connection> {
    let conn = Connection::open_in_memory()?;
    configure_memory(&conn)?;
    Ok(conn)
}

fn configure_memory(conn: &Connection) -> AppResult<()> {
    restore_validation::harden_connection(conn)?;
    conn.execute_batch(
        "PRAGMA temp_store=MEMORY; PRAGMA journal_mode=MEMORY; PRAGMA synchronous=FULL;",
    )?;
    Ok(())
}

/// 仅用于已有镜像；空的新库由可信迁移初始化，无需伪装成历史备份。
fn validate_and_upgrade(conn: &mut Connection) -> AppResult<(i64, i64, i64)> {
    restore_validation::harden_connection(conn)?;
    let limits = limits();
    let mut budget = SqlBudget::new(conn, &limits)?;
    restore_validation::inspect_backup(&budget, &limits)?;
    configure_memory(&budget)?;
    migrations::apply(&mut budget)?;
    restore_validation::validate_database(&budget, &limits)
}

fn read_plain_backup(path: &Path) -> AppResult<Connection> {
    restore_validation::check_file_size(path, &limits())?;
    let mut source = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    restore_validation::harden_connection(&source)?;
    source.busy_timeout(Duration::from_millis(250))?;
    let budget = SqlBudget::new(&mut source, &limits())?;
    budget.execute_batch("BEGIN;")?;
    restore_validation::inspect_backup(&budget, &limits())?;
    let mut staged = memory_connection()?;
    copy_database(&budget, &mut staged)?;
    drop(budget);
    drop(source);
    validate_and_upgrade(&mut staged)?;
    // SQLite Backup 可把 WAL 格式头带入内存。仅在已验证的内存副本上重建，
    // 生成可独立 deserialize 的普通镜像；原备份保持只读且不被 checkpoint。
    {
        let budget = SqlBudget::new(&mut staged, &limits())?;
        // VACUUM 自身需要一个内部附加库；输入结构已通过白名单，唯一执行的
        // 语句是本地固定 SQL。内存主库和 temp_store=MEMORY 保证重建仍在内存。
        budget.set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_ATTACHED, 1)?;
        budget.execute_batch("VACUUM;")?;
        budget.set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_ATTACHED, 0)?;
        restore_validation::validate_database(&budget, &limits())?;
    }
    Ok(staged)
}

/// 旧实例没有新的 .vault.lock；Windows 用实际共享模式探测，发布时操作系统
/// 还会再次检查目标是否允许替换。探测句柄不能跨发布持有，否则会拦截自身。
fn probe_legacy_exclusive(path: &Path) -> AppResult<()> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let _probe = OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(path)
            .map_err(|_| {
                AppError::Conflict("旧数据库正在使用或不可独占访问，请关闭其他实例后再迁移".into())
            })?;
    }
    #[cfg(not(windows))]
    {
        let _ = path;
    }
    Ok(())
}

fn convert_legacy_to_memory(path: &Path) -> AppResult<(Connection, SourceLease)> {
    probe_legacy_exclusive(path)?;
    restore_validation::check_file_size(path, &limits())?;
    let mut source = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    restore_validation::harden_connection(&source)?;
    source.busy_timeout(Duration::from_millis(250))?;
    let budget = SqlBudget::new(&mut source, &limits())?;
    // 结构白名单在任何迁移、checkpoint 或业务读取之前执行。
    restore_validation::inspect_backup(&budget, &limits())?;
    budget.execute_batch("PRAGMA locking_mode=EXCLUSIVE; BEGIN EXCLUSIVE; COMMIT;")?;
    let (busy, log, checkpointed): (i64, i64, i64) =
        budget.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
    if busy != 0 || (log >= 0 && checkpointed < log) {
        return Err(AppError::Conflict(
            "旧数据库 WAL 尚未安全写回，请关闭其他实例后重试".into(),
        ));
    }
    let mode: String = budget.query_row("PRAGMA journal_mode=DELETE", [], |row| row.get(0))?;
    if !mode.eq_ignore_ascii_case("delete") {
        return Err(AppError::Conflict(
            "旧数据库日志仍被占用，未执行加密替换".into(),
        ));
    }
    let mut staged = memory_connection()?;
    copy_database(&budget, &mut staged)?;
    // SQLite exclusive 仍有效时捕获固定指纹，之后的防写租约必须匹配它。
    let expected = source_fingerprint(&mut File::open(path)?)?;
    drop(budget);
    source.close().map_err(|(_, error)| AppError::Db(error))?;
    let lease = SourceLease::acquire_verified(path, &expected)?;
    // 禁止遗漏关闭后重新出现的 WAL/热日志，也不擅自删除其内容。
    for suffix in ["-wal", "-journal"] {
        let sidecar = sidecar_path(path, suffix);
        match fs::metadata(sidecar) {
            Ok(metadata) if metadata.len() != 0 => {
                return Err(AppError::Conflict(
                    "旧数据库日志发生变化，未执行加密替换".into(),
                ))
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    validate_and_upgrade(&mut staged)?;
    Ok((staged, lease))
}

fn db_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("db"))
}

impl Db {
    pub fn new_vault(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        let instance_lock = vault::lock_vault(path)?;
        Ok(Self {
            conn: Mutex::new(memory_connection()?),
            path: path.to_path_buf(),
            schema_version: AtomicI64::new(0),
            requires_vault: true,
            unlocked: AtomicBool::new(false),
            vault: Mutex::new(None),
            _instance_lock: Some(instance_lock),
        })
    }

    pub fn vault_status(&self) -> AppResult<VaultStatus> {
        let exists = self.path.try_exists()?;
        let configured = exists && vault::is_encrypted(&self.path)?;
        Ok(VaultStatus {
            configured,
            unlocked: self.unlocked.load(Ordering::Acquire),
            legacy: exists && !configured,
        })
    }

    pub fn unlock_vault(
        &self,
        password: &str,
        confirm: Option<&str>,
        migrate_legacy: bool,
    ) -> AppResult<()> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| AppError::Other("数据库连接状态异常，请重启应用".into()))?;
        if self.unlocked.load(Ordering::Acquire) {
            return Err(AppError::Conflict("数据库已解锁".into()));
        }
        let mut session_guard = self
            .vault
            .lock()
            .map_err(|_| AppError::Other("加密会话状态异常，请重启应用".into()))?;
        let status = self.vault_status()?;
        let (session, mut staged, mut legacy_lease) = if status.configured {
            let (session, mut staged) = VaultSession::open(&self.path, password)?;
            validate_and_upgrade(&mut staged)?;
            (session, staged, None)
        } else {
            if status.legacy && !migrate_legacy {
                return Err(AppError::validation(
                    "发现旧版明文数据库，请明确确认加密迁移后继续",
                ));
            }
            vault::validate_new_password(password, confirm)?;
            let session = VaultSession::new(password)?;
            let (staged, lease) = if status.legacy {
                let (staged, lease) = convert_legacy_to_memory(&self.path)?;
                (staged, Some(lease))
            } else {
                let mut staged = memory_connection()?;
                migrations::apply(&mut staged)?;
                let budget = SqlBudget::new(&mut staged, &limits())?;
                restore_validation::validate_database(&budget, &limits())?;
                drop(budget);
                (staged, None)
            };
            (session, staged, lease)
        };
        if let Some(lease) = legacy_lease.as_mut() {
            lease.verify_unchanged(&self.path)?;
        }
        // 首次创建不能覆盖竞态出现的文件；旧库及已认证密文才允许替换。
        session.write(&mut staged, &self.path, status.configured || status.legacy)?;
        drop(legacy_lease);
        let version = migrations::current_version(&staged)?;
        *conn = staged;
        *session_guard = Some(session);
        self.set_schema_version(version);
        self.unlocked.store(true, Ordering::Release);
        Ok(())
    }

    pub(super) fn persist_if_changed(
        &self,
        conn: &mut Connection,
        before: (u64, i64),
    ) -> AppResult<()> {
        if !self.requires_vault {
            return Ok(());
        }
        let mut session_guard = self.vault.lock().map_err(|_| {
            self.unlocked.store(false, Ordering::Release);
            AppError::Other("加密会话状态异常，已锁定，请重启应用".into())
        })?;
        let session = session_guard.as_ref().ok_or_else(|| {
            self.unlocked.store(false, Ordering::Release);
            AppError::validation("请先输入口令解锁本地数据")
        })?;
        let result: AppResult<()> = (|| {
            if !conn.is_autocommit() {
                return Err(AppError::Other("数据库操作尚未提交，未保存此次更改".into()));
            }
            let schema: i64 = conn.query_row("PRAGMA schema_version", [], |row| row.get(0))?;
            if conn.total_changes() == before.0 && schema == before.1 {
                return Ok(());
            }
            // 自己写出的文件必须同样满足下次解锁的所有边界，而不只检查 DDL。
            {
                let budget = SqlBudget::new(&mut *conn, &limits())?;
                restore_validation::validate_database(&budget, &limits())?;
            }
            session.write(conn, &self.path, true)?;
            Ok(())
        })();
        if let Err(error) = result {
            match self.reload_persisted(conn, &mut session_guard) {
                Ok(()) => {
                    return Err(AppError::Other(format!(
                        "保存失败，已恢复至上次成功保存的数据：{error}"
                    )));
                }
                Err(reload_error) => {
                    return Err(AppError::Other(format!("保存失败且无法重新载入，已锁定以保护数据，请重启应用：{error}；{reload_error}")));
                }
            }
        }
        Ok(())
    }

    fn reload_persisted(
        &self,
        conn: &mut Connection,
        session: &mut Option<VaultSession>,
    ) -> AppResult<()> {
        let reload: AppResult<(Connection, i64)> = (|| {
            let mut previous = session
                .as_ref()
                .ok_or_else(|| AppError::validation("加密会话未解锁"))?
                .load_current(&self.path)?;
            let summary = validate_and_upgrade(&mut previous)?;
            Ok((previous, summary.0))
        })();
        match reload {
            Ok((previous, version)) => {
                *conn = previous;
                self.set_schema_version(version);
                Ok(())
            }
            Err(error) => {
                self.unlocked.store(false, Ordering::Release);
                *session = None;
                self.set_schema_version(0);
                if let Ok(empty) = memory_connection() {
                    *conn = empty;
                }
                Err(error)
            }
        }
    }

    /// 闭包报错时不保存其中已提交的局部写入，而是回到最后成功的密文镜像。
    pub(super) fn rollback_failed_operation(
        &self,
        conn: &mut Connection,
        before: (u64, i64),
    ) -> AppResult<()> {
        if !self.requires_vault {
            return Ok(());
        }
        let unchanged = conn.is_autocommit()
            && conn.total_changes() == before.0
            && conn
                .query_row("PRAGMA schema_version", [], |row| row.get::<_, i64>(0))
                .is_ok_and(|version| version == before.1);
        if unchanged {
            return Ok(());
        }
        let mut session = self.vault.lock().map_err(|_| {
            self.unlocked.store(false, Ordering::Release);
            AppError::Other("加密会话状态异常，已锁定，请重启应用".into())
        })?;
        self.reload_persisted(conn, &mut session).map_err(|error| {
            AppError::Other(format!(
                "操作失败且无法恢复上次保存的数据，已锁定，请重启应用：{error}"
            ))
        })
    }

    pub(super) fn restore_secure(
        &self,
        source: &Path,
        password: Option<&str>,
    ) -> AppResult<RestoreSummary> {
        let source = source.canonicalize()?;
        if !source.is_file() {
            return Err(AppError::validation("恢复来源必须是文件"));
        }
        if source == self.path.canonicalize()? {
            return Err(AppError::validation("不能用正在使用的数据库文件恢复自身"));
        }
        let mut conn = self.lock()?;
        let session_guard = self
            .vault
            .lock()
            .map_err(|_| AppError::Other("加密会话状态异常，请重启应用".into()))?;
        let session = session_guard
            .as_ref()
            .ok_or_else(|| AppError::validation("请先解锁本地数据"))?;
        let mut staged = if vault::is_encrypted(&source)? {
            let mut staged = session.read_backup(&source, password)?;
            validate_and_upgrade(&mut staged)?;
            staged
        } else {
            read_plain_backup(&source)?
        };
        let summary = {
            let budget = SqlBudget::new(&mut staged, &limits())?;
            restore_validation::validate_database(&budget, &limits())?
        };
        fs::create_dir_all(self.backups_dir())?;
        let snapshot = self.backups_dir().join(format!(
            "pre-restore-{}-{}.snbak",
            chrono::Local::now().format("%Y%m%d-%H%M%S"),
            uuid::Uuid::new_v4()
        ));
        session.write(&mut conn, &snapshot, false)?;
        // 先原子保存新密文，最后才交换内存连接；任一失败不改当前会话数据。
        session.write(&mut staged, &self.path, true)?;
        *conn = staged;
        self.set_schema_version(summary.0);
        Ok(RestoreSummary {
            source_path: source.display().to_string(),
            snapshot_path: snapshot.display().to_string(),
            schema_version: summary.0,
            task_count: summary.1,
            update_count: summary.2,
        })
    }

    /// 未识别/无效 .db 也计入待处理数量，不把枚举失败伪装为零。
    pub fn plaintext_backup_count(&self) -> AppResult<u32> {
        let entries = match fs::read_dir(self.backups_dir()) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(error.into()),
        };
        let mut count = 0_u32;
        for entry in entries {
            let entry = entry?;
            let kind = entry.file_type()?;
            if db_extension(&entry.path())
                && (kind.is_symlink() || (kind.is_file() && !vault::is_encrypted(&entry.path())?))
            {
                count = count.saturating_add(1);
            }
        }
        Ok(count)
    }

    /// 用户明确触发后才转换旧备份。每个失败保留原件，其余继续处理。
    /// 返回（本次成功数，目录内剩余明文 .db 数）。
    pub fn encrypt_legacy_backups(&self) -> AppResult<(u32, u32)> {
        let _conn = self.lock()?;
        let session_guard = self
            .vault
            .lock()
            .map_err(|_| AppError::Other("加密会话状态异常，请重启应用".into()))?;
        let session = session_guard
            .as_ref()
            .ok_or_else(|| AppError::validation("请先解锁本地数据"))?;
        let entries = match fs::read_dir(self.backups_dir()) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok((0, 0)),
            Err(error) => return Err(error.into()),
        };
        let mut converted = 0_u32;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if !db_extension(&path) || !entry.file_type()?.is_file() {
                continue;
            }
            let attempt: AppResult<()> = (|| {
                let mut lease = SourceLease::acquire(&path)?;
                if vault::is_encrypted(&path)? {
                    return Ok(());
                }
                // 非静态备份可能仍有写者；不删除任何附属日志或正在使用的文件。
                if ["-wal", "-journal"]
                    .iter()
                    .any(|suffix| sidecar_path(&path, suffix).exists())
                {
                    return Err(AppError::Conflict(
                        "备份有活动日志，请先关闭写入它的程序".into(),
                    ));
                }
                let mut staged = read_plain_backup(&path)?;
                let name = path.file_stem().unwrap_or_default().to_string_lossy();
                let encrypted =
                    path.with_file_name(format!("{name}-{}.snbak", uuid::Uuid::new_v4()));
                session.write(&mut staged, &encrypted, false)?;
                let mut verified = session.read_backup(&encrypted, None)?;
                validate_and_upgrade(&mut verified)?;
                lease.verify_unchanged(&path)?;
                fs::remove_file(&path)?;
                drop(lease);
                converted = converted.saturating_add(1);
                Ok(())
            })();
            if let Err(error) = attempt {
                eprintln!("[shiftnote] 旧备份未转换 {}：{error}", path.display());
            }
        }
        Ok((converted, self.plaintext_backup_count()?))
    }
}
