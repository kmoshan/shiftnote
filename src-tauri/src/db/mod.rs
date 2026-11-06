//! 数据库连接管理与公共工具。

pub mod batch;
pub mod handovers;
pub mod migrations;
pub(super) mod restore_validation;
pub mod settings;
pub mod tasks;
pub mod updates;
mod vault_storage;

#[cfg(test)]
mod restore_tests;
#[cfg(test)]
mod vault_tests;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use chrono::{Datelike, Duration as ChronoDuration, Local, NaiveDate, TimeZone};
use rusqlite::{Connection, OpenFlags};

use crate::error::{AppError, AppResult};
use crate::models::RestoreSummary;

/// 统一时间基准：Unix 毫秒时间戳（UTC）。
pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// 返回本地某一天（offset_days=0 表示今天）的起止毫秒时间戳，左闭右开。
pub fn local_day_bounds(offset_days: i64) -> (i64, i64) {
    let today: NaiveDate = Local::now().date_naive();
    let target = today + ChronoDuration::days(offset_days);
    (
        day_start_ms(target),
        day_start_ms(target + ChronoDuration::days(1)),
    )
}

pub fn day_start_ms(date: NaiveDate) -> i64 {
    let naive = date.and_hms_opt(0, 0, 0).expect("00:00:00 一定合法");
    Local
        .from_local_datetime(&naive)
        .earliest()
        .map(|dt| dt.timestamp_millis())
        .unwrap_or_else(|| naive.and_utc().timestamp_millis())
}

pub fn today_string() -> String {
    let now = Local::now();
    format!("{:04}-{:02}-{:02}", now.year(), now.month(), now.day())
}

pub struct OpenOutcome {
    pub db: Db,
    /// 数据库损坏时，原始文件被重命名保存到的路径
    pub recovered_backup: Option<PathBuf>,
}

pub struct Db {
    conn: Mutex<Connection>,
    path: PathBuf,
    schema_version: AtomicI64,
    requires_vault: bool,
    unlocked: AtomicBool,
    vault: Mutex<Option<vault_storage::StorageSession>>,
    _instance_lock: Option<std::fs::File>,
}

impl Db {
    /// 打开数据库；若文件损坏则自动备份旧文件并重建，保证应用仍可启动。
    ///
    /// 只有“确认为损坏”时才重建：如果是文件被占用、权限不足等错误，
    /// 直接向上返回，避免把正常数据库误重命名（例如同时打开了两个实例）。
    pub fn open_or_recover(path: &Path) -> AppResult<OpenOutcome> {
        match Self::try_open(path) {
            Ok(db) => Ok(OpenOutcome {
                db,
                recovered_backup: None,
            }),
            Err(first_error) => {
                if !path.exists() || !is_corruption(&first_error) {
                    return Err(first_error);
                }

                let stamp = Local::now().format("%Y%m%d-%H%M%S").to_string();
                let backup = path.with_file_name(format!("shiftnote.corrupt-{stamp}.db"));
                std::fs::rename(path, &backup)?;

                // 清理可能残留的 WAL / SHM 文件，避免它们污染新库
                let _ = std::fs::remove_file(sidecar_path(path, "-wal"));
                let _ = std::fs::remove_file(sidecar_path(path, "-shm"));

                match Self::try_open(path) {
                    Ok(db) => {
                        eprintln!(
                            "[shiftnote] 数据库无法打开（{}），已备份到 {} 并重建",
                            first_error,
                            backup.display()
                        );
                        Ok(OpenOutcome {
                            db,
                            recovered_backup: Some(backup),
                        })
                    }
                    Err(second) => Err(second),
                }
            }
        }
    }

    fn try_open(path: &Path) -> AppResult<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }

        let mut conn = Connection::open(path)?;
        restore_validation::harden_connection(&conn)?;

        // 完整性快速自检：损坏文件会在此时暴露，而不是在使用中随机报错
        let check: String = conn.query_row("PRAGMA quick_check(1)", [], |row| row.get(0))?;
        if !check.eq_ignore_ascii_case("ok") {
            return Err(AppError::Other(format!("数据库文件校验失败：{check}")));
        }

        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;
             PRAGMA temp_store = MEMORY;",
        )?;

        let schema_version = migrations::apply(&mut conn)?;

        Ok(Self {
            conn: Mutex::new(conn),
            path: path.to_path_buf(),
            schema_version: AtomicI64::new(schema_version),
            requires_vault: false,
            unlocked: AtomicBool::new(true),
            vault: Mutex::new(None),
            _instance_lock: None,
        })
    }

    #[cfg(test)]
    pub fn open_in_memory() -> AppResult<Self> {
        let mut conn = Connection::open_in_memory()?;
        restore_validation::harden_connection(&conn)?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        let schema_version = migrations::apply(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            path: PathBuf::from(":memory:"),
            schema_version: AtomicI64::new(schema_version),
            requires_vault: false,
            unlocked: AtomicBool::new(true),
            vault: Mutex::new(None),
            _instance_lock: None,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn schema_version(&self) -> i64 {
        self.schema_version.load(Ordering::Relaxed)
    }

    fn set_schema_version(&self, version: i64) {
        self.schema_version.store(version, Ordering::Relaxed);
    }

    fn lock(&self) -> AppResult<MutexGuard<'_, Connection>> {
        let guard = self
            .conn
            .lock()
            .map_err(|_| AppError::Other("数据库连接状态异常，请重启应用".into()))?;
        if !self.unlocked.load(Ordering::Acquire) {
            return Err(AppError::validation("请先完成首次设置或解锁本地数据"));
        }
        Ok(guard)
    }

    pub fn with_conn<T>(&self, f: impl FnOnce(&Connection) -> AppResult<T>) -> AppResult<T> {
        let mut guard = self.lock()?;
        if !self.requires_vault {
            return f(&guard);
        }
        let before = change_marker(&guard)?;
        let result = f(&guard);
        match result {
            Ok(value) => {
                self.persist_if_changed(&mut guard, before)?;
                Ok(value)
            }
            Err(error) => {
                self.rollback_failed_operation(&mut guard, before)?;
                Err(error)
            }
        }
    }

    pub fn with_conn_mut<T>(
        &self,
        f: impl FnOnce(&mut Connection) -> AppResult<T>,
    ) -> AppResult<T> {
        let mut guard = self.lock()?;
        if !self.requires_vault {
            return f(&mut guard);
        }
        let before = change_marker(&guard)?;
        let result = f(&mut guard);
        match result {
            Ok(value) => {
                self.persist_if_changed(&mut guard, before)?;
                Ok(value)
            }
            Err(error) => {
                self.rollback_failed_operation(&mut guard, before)?;
                Err(error)
            }
        }
    }

    /// 备份目录：与数据库文件同级。
    pub fn backups_dir(&self) -> PathBuf {
        self.path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("backups")
    }

    /// 使用 SQLite 在线备份 API 复制当前数据库到目标文件。
    pub fn backup_to(&self, dest: &Path) -> AppResult<u64> {
        if let Some(dir) = dest.parent() {
            std::fs::create_dir_all(dir)?;
        }
        {
            let mut guard = self.lock()?;
            if self.requires_vault {
                let session = self
                    .vault
                    .lock()
                    .map_err(|_| AppError::Other("加密会话状态异常，请重启应用".into()))?;
                return session
                    .as_ref()
                    .ok_or_else(|| AppError::validation("请先解锁本地数据"))?
                    .write(&mut guard, dest, false);
            }
            guard.backup(
                rusqlite::MAIN_DB,
                dest,
                None::<fn(rusqlite::backup::Progress)>,
            )?;
        }
        Ok(std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0))
    }

    /// 在独立副本上迁移和验证后恢复；同一个连接锁覆盖快照、覆盖与回滚。
    pub fn restore_from(&self, source: &Path) -> AppResult<RestoreSummary> {
        if self.requires_vault {
            return self.restore_secure(source, None);
        }
        self.restore_from_using(source, copy_database)
    }

    pub fn restore_with_password(
        &self,
        source: &Path,
        password: Option<&str>,
    ) -> AppResult<RestoreSummary> {
        if self.requires_vault {
            self.restore_secure(source, password)
        } else {
            self.restore_from(source)
        }
    }

    fn restore_from_using(
        &self,
        source: &Path,
        copy: impl FnMut(&Connection, &mut Connection) -> AppResult<()>,
    ) -> AppResult<RestoreSummary> {
        self.restore_from_with_limits(source, &Default::default(), copy)
    }

    fn restore_from_with_limits(
        &self,
        source: &Path,
        limits: &restore_validation::RestoreLimits,
        mut copy: impl FnMut(&Connection, &mut Connection) -> AppResult<()>,
    ) -> AppResult<RestoreSummary> {
        let source_path = source
            .canonicalize()
            .unwrap_or_else(|_| source.to_path_buf());
        if !source_path.is_file() {
            return Err(AppError::not_found("备份文件不存在，或不是一个文件"));
        }
        let live_path = self
            .path
            .canonicalize()
            .unwrap_or_else(|_| self.path.clone());
        if source_path == live_path {
            return Err(AppError::validation(
                "不能使用正在运行的数据库文件作为恢复来源",
            ));
        }

        restore_validation::check_file_size(&source_path, limits)?;
        let mut guard = self.lock()?;
        let mut source_conn =
            Connection::open_with_flags(&source_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        restore_validation::harden_connection(&source_conn)?;
        let source_budget = restore_validation::SqlBudget::new(&mut source_conn, limits)?;
        // 固定同一个只读快照，避免校验与复制之间来源被并发改写。
        source_budget.execute_batch("BEGIN;")?;
        restore_validation::inspect_backup(&source_budget, limits)
            .map_err(|error| AppError::validation(format!("备份预检失败：{error}")))?;
        let backups = self.backups_dir();
        std::fs::create_dir_all(&backups)?;
        let temporary =
            TemporaryDatabase(backups.join(format!("restore-check-{}.db", uuid::Uuid::new_v4())));
        let mut staged = Connection::open(&temporary.0)?;
        restore_validation::harden_connection(&staged)?;
        copy_database(&source_budget, &mut staged)?;
        drop(source_budget);
        drop(source_conn);
        {
            let mut budget = restore_validation::SqlBudget::new(&mut staged, limits)?;
            restore_validation::inspect_backup(&budget, limits)?;
            migrations::apply(&mut budget)
                .map_err(|error| AppError::validation(format!("备份迁移失败：{error}")))?;
            restore_validation::validate_database(&budget, limits)
                .map_err(|error| AppError::validation(format!("备份校验失败：{error}")))?;
        }

        let stamp = Local::now().format("%Y%m%d-%H%M%S");
        let snapshot = backups.join(format!("pre-restore-{stamp}-{}.db", uuid::Uuid::new_v4()));
        guard.backup(
            rusqlite::MAIN_DB,
            &snapshot,
            None::<fn(rusqlite::backup::Progress)>,
        )?;
        let result: AppResult<(i64, i64, i64)> = (|| {
            copy(&staged, &mut guard)?;
            guard.execute_batch("PRAGMA foreign_keys=ON;")?;
            let summary = {
                let budget = restore_validation::SqlBudget::new(&mut guard, limits)?;
                restore_validation::validate_database(&budget, limits)?
            };
            guard.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
            Ok(summary)
        })();
        let (schema_version, task_count, update_count) = match result {
            Ok(summary) => summary,
            Err(error) => {
                let rollback: AppResult<()> = (|| {
                    let previous =
                        Connection::open_with_flags(&snapshot, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
                    restore_validation::harden_connection(&previous)?;
                    copy(&previous, &mut guard)?;
                    validate_database(&mut guard)?;
                    Ok(())
                })();
                let foreign_keys = guard.execute_batch("PRAGMA foreign_keys=ON;");
                if let Err(rollback_error) = rollback {
                    return Err(AppError::Other(format!(
                        "恢复失败：{error}；回滚也失败：{rollback_error}。恢复前快照保留于 {}{}",
                        snapshot.display(),
                        foreign_keys
                            .err()
                            .map(|e| format!("；外键重新启用失败：{e}"))
                            .unwrap_or_default()
                    )));
                }
                foreign_keys?;
                return Err(AppError::Other(format!("恢复失败并已回滚：{error}")));
            }
        };
        self.set_schema_version(schema_version);
        drop(staged);
        drop(temporary);
        Ok(RestoreSummary {
            source_path: source_path.display().to_string(),
            snapshot_path: snapshot.display().to_string(),
            task_count,
            update_count,
            schema_version,
        })
    }

    pub fn file_size(&self) -> u64 {
        std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0)
    }

    pub fn wal_enabled(&self) -> bool {
        self.with_conn(|conn| {
            let mode: String = conn.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
            Ok(mode.eq_ignore_ascii_case("wal"))
        })
        .unwrap_or(false)
    }
}

fn change_marker(conn: &Connection) -> AppResult<(u64, i64)> {
    Ok((
        conn.total_changes(),
        conn.query_row("PRAGMA schema_version", [], |row| row.get(0))?,
    ))
}

/// foreign_keys 的恢复位于闭包外，Backup::new 失败也不能绕过。
fn copy_database(source: &Connection, destination: &mut Connection) -> AppResult<()> {
    destination.execute_batch("PRAGMA foreign_keys=OFF;")?;
    let result: AppResult<()> = (|| {
        let backup = rusqlite::backup::Backup::new(source, destination)?;
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            if std::time::Instant::now() >= deadline {
                return Err(AppError::validation("数据库复制超过 30 秒执行预算"));
            }
            match backup.step(256)? {
                rusqlite::backup::StepResult::Done => return Ok(()),
                rusqlite::backup::StepResult::More => {}
                _ => std::thread::sleep(Duration::from_millis(5)),
            }
        }
    })();
    let enabled = destination.execute_batch("PRAGMA foreign_keys=ON;");
    match (result, enabled) {
        (Err(error), Err(enable_error)) => Err(AppError::Other(format!(
            "{error}；重新启用外键失败：{enable_error}"
        ))),
        (Err(error), _) => Err(error),
        (_, Err(error)) => Err(error.into()),
        _ => Ok(()),
    }
}

struct TemporaryDatabase(PathBuf);
impl Drop for TemporaryDatabase {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_file(sidecar_path(&self.0, "-wal"));
        let _ = std::fs::remove_file(sidecar_path(&self.0, "-shm"));
    }
}

fn sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

/// 判断错误是否属于“数据库文件损坏”，只有这类错误才值得重建数据库。
fn is_corruption(err: &AppError) -> bool {
    match err {
        AppError::Db(rusqlite::Error::SqliteFailure(inner, _)) => matches!(
            inner.code,
            rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase
        ),
        AppError::Other(message) => message.contains("校验失败"),
        _ => false,
    }
}

/// 最新库的完整校验；仅用于覆盖后的验证和失败回滚。
fn validate_database(conn: &mut Connection) -> AppResult<(i64, i64, i64)> {
    let limits = restore_validation::RestoreLimits::default();
    let budget = restore_validation::SqlBudget::new(conn, &limits)?;
    restore_validation::validate_database(&budget, &limits)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_to_latest_version() {
        let db = Db::open_in_memory().expect("内存数据库应能初始化");
        assert_eq!(db.schema_version(), migrations::latest_version());

        let tables: Vec<String> = db
            .with_conn(|conn| {
                let mut stmt = conn
                    .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")?;
                let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
                Ok(rows.collect::<Result<Vec<_>, _>>()?)
            })
            .unwrap();

        for expected in [
            "tasks",
            "task_updates",
            "reminders",
            "handovers",
            "handover_tasks",
            "categories",
            "settings",
        ] {
            assert!(tables.contains(&expected.to_string()), "缺少表 {expected}");
        }
    }

    #[test]
    fn day_bounds_are_ordered_and_cover_24h() {
        let (start, end) = local_day_bounds(0);
        assert!(start < end);
        // 允许夏令时造成的 1 小时偏差
        let span_hours = (end - start) / 3_600_000;
        assert!(
            (23..=25).contains(&span_hours),
            "跨度异常：{span_hours} 小时"
        );
    }

    #[test]
    fn only_corruption_triggers_recovery() {
        let busy = AppError::Db(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error {
                code: rusqlite::ErrorCode::DatabaseBusy,
                extended_code: 5,
            },
            Some("database is locked".into()),
        ));
        assert!(!is_corruption(&busy), "占用/锁冲突不能触发重建");

        let unreadable = AppError::Io(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "denied",
        ));
        assert!(!is_corruption(&unreadable));

        let corrupt = AppError::Db(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error {
                code: rusqlite::ErrorCode::DatabaseCorrupt,
                extended_code: 11,
            },
            None,
        ));
        assert!(is_corruption(&corrupt));

        assert!(is_corruption(&AppError::Other(
            "数据库文件校验失败：bad".into()
        )));
    }

    #[test]
    fn migrates_v1_database_and_backfills_department_and_recorded_at() {
        let dir = std::env::temp_dir().join(format!("shiftnote-v1-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("shiftnote.db");

        // 构造一个 v1 结构的旧库，带旧字段（负责人/记录人/交接对象/预计完成）
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(include_str!("../../migrations/001_init.sql"))
                .unwrap();
            conn.execute_batch("PRAGMA user_version = 1;").unwrap();
            conn.execute(
                "INSERT INTO tasks
                 (id, title, priority, status, hidden, archived, assignee, recorder,
                  handover_to, due_at, sort_order, created_at, updated_at)
                 VALUES ('legacy-1', '旧版本事项', 1, 'open', 0, 0, '张伟', '李静',
                         '王强', 5555, 10, 1000, 1000)",
                [],
            )
            .unwrap();
        }

        let outcome = Db::open_or_recover(&path).expect("旧库应能自动升级");
        assert_eq!(outcome.db.schema_version(), migrations::latest_version());

        let task = outcome
            .db
            .with_conn(|conn| crate::db::tasks::get(conn, "legacy-1"))
            .unwrap()
            .expect("旧数据必须保留");

        assert_eq!(task.title, "旧版本事项");
        assert_eq!(
            task.department.as_deref(),
            Some("张伟"),
            "按 assignee 回填部门"
        );
        assert_eq!(task.recorded_at, 1000, "用 created_at 回填记录时间");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detects_corrupt_database_and_recovers() {
        let dir = std::env::temp_dir().join(format!("shiftnote-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("shiftnote.db");
        std::fs::write(&path, b"this is definitely not a sqlite database").unwrap();

        let outcome = Db::open_or_recover(&path).expect("损坏数据库应能自动恢复");
        assert!(outcome.recovered_backup.is_some());
        assert!(outcome
            .db
            .with_conn(|c| Ok(
                c.query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get::<_, i64>(0))?
            ))
            .is_ok());

        let _ = std::fs::remove_dir_all(&dir);
    }

    fn task_count(db: &Db) -> i64 {
        db.with_conn(|conn| Ok(conn.query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0))?))
            .unwrap()
    }

    #[test]
    fn restores_from_backup_and_keeps_pre_restore_snapshot() {
        let dir = std::env::temp_dir().join(format!("shiftnote-restore-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        // 源库：写入一条事项后备份
        let source = Db::open_or_recover(&dir.join("source.db")).unwrap().db;
        source
            .with_conn(|conn| {
                crate::db::tasks::create(
                    conn,
                    &crate::models::CreateTaskInput {
                        title: "备份里的事项".into(),
                        department: Some("运维一部".into()),
                        ..Default::default()
                    },
                )
            })
            .unwrap();
        let backup_path = dir.join("backup.db");
        source.backup_to(&backup_path).unwrap();
        assert!(backup_path.exists());

        // 目标库：本来没有数据
        let target = Db::open_or_recover(&dir.join("target.db")).unwrap().db;
        assert_eq!(task_count(&target), 0);

        let summary = target.restore_from(&backup_path).unwrap();
        assert_eq!(summary.task_count, 1);
        assert_eq!(summary.schema_version, migrations::latest_version());
        assert_eq!(task_count(&target), 1);
        assert!(
            std::path::Path::new(&summary.snapshot_path).exists(),
            "恢复前必须自动生成快照"
        );

        // 恢复后仍可正常读写
        target
            .with_conn(|conn| {
                crate::db::tasks::create(
                    conn,
                    &crate::models::CreateTaskInput {
                        title: "恢复后新增".into(),
                        ..Default::default()
                    },
                )
            })
            .unwrap();
        assert_eq!(task_count(&target), 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_backup_that_is_not_shiftnote() {
        let dir =
            std::env::temp_dir().join(format!("shiftnote-badrestore-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        let junk = dir.join("junk.db");
        {
            let conn = Connection::open(&junk).unwrap();
            conn.execute_batch("CREATE TABLE foo (id INTEGER);")
                .unwrap();
        }

        let target = Db::open_or_recover(&dir.join("target.db")).unwrap().db;
        target
            .with_conn(|conn| {
                crate::db::tasks::create(
                    conn,
                    &crate::models::CreateTaskInput {
                        title: "原有数据".into(),
                        ..Default::default()
                    },
                )
            })
            .unwrap();

        let error = target.restore_from(&junk).unwrap_err();
        assert!(matches!(error, AppError::Validation(_)));
        assert_eq!(task_count(&target), 1, "非法备份不能影响现有数据");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
