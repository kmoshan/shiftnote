//! 数据库连接管理与公共工具。

pub mod migrations;
pub mod settings;
pub mod tasks;
pub mod updates;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
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
    let naive = date
        .and_hms_opt(0, 0, 0)
        .expect("00:00:00 一定合法");
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
        })
    }

    #[cfg(test)]
    pub fn open_in_memory() -> AppResult<Self> {
        let mut conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        let schema_version = migrations::apply(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            path: PathBuf::from(":memory:"),
            schema_version: AtomicI64::new(schema_version),
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
        self.conn
            .lock()
            .map_err(|_| AppError::Other("数据库连接状态异常，请重启应用".into()))
    }

    pub fn with_conn<T>(&self, f: impl FnOnce(&Connection) -> AppResult<T>) -> AppResult<T> {
        let guard = self.lock()?;
        f(&guard)
    }

    pub fn with_conn_mut<T>(&self, f: impl FnOnce(&mut Connection) -> AppResult<T>) -> AppResult<T> {
        let mut guard = self.lock()?;
        f(&mut guard)
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
            let guard = self.lock()?;
            guard.backup(rusqlite::MAIN_DB, dest, None::<fn(rusqlite::backup::Progress)>)?;
        }
        Ok(std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0))
    }

    /// 从备份文件恢复数据。
    ///
    /// 流程：预检（完整性 / 版本 / 是否本应用的库）→ 快照当前数据 →
    /// 用 SQLite backup API 覆盖在线连接 → 补齐迁移 → 外键一致性校验。
    /// 任一步失败都会尝试用快照回滚，保证不会把库改成半截状态。
    pub fn restore_from(&self, source: &Path) -> AppResult<RestoreSummary> {
        let source_path = source
            .canonicalize()
            .unwrap_or_else(|_| source.to_path_buf());

        if !source_path.is_file() {
            return Err(AppError::not_found("备份文件不存在，或不是一个文件"));
        }
        if source_path == self.path {
            return Err(AppError::validation(
                "不能使用正在运行的数据库文件作为恢复来源",
            ));
        }

        inspect_backup(&source_path)?;

        // 覆盖前先保留当前数据，便于回退
        let stamp = Local::now().format("%Y%m%d-%H%M%S").to_string();
        let snapshot = self.backups_dir().join(format!("pre-restore-{stamp}.db"));
        self.backup_to(&snapshot)?;

        if let Err(error) = self.copy_from(&source_path) {
            let _ = self.copy_from(&snapshot);
            return Err(error);
        }

        // 旧备份可能是低版本结构，补齐迁移
        let schema_version = match self.with_conn_mut(|conn| migrations::apply(conn)) {
            Ok(version) => version,
            Err(error) => {
                let _ = self.copy_from(&snapshot);
                return Err(error);
            }
        };

        // 外键一致性校验：备份内部若有悬挂引用，直接回滚
        let broken = self.with_conn(|conn| {
            Ok(conn.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get::<_, i64>(0)
            })?)
        })?;
        if broken > 0 {
            let _ = self.copy_from(&snapshot);
            return Err(AppError::validation(format!(
                "备份中存在 {broken} 处关联数据不一致，已回滚到恢复前的数据"
            )));
        }

        let _ = self.with_conn(|conn| Ok(conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?));
        self.set_schema_version(schema_version);

        let (task_count, update_count) = self.with_conn(|conn| {
            let tasks: i64 = conn.query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get(0))?;
            let updates: i64 =
                conn.query_row("SELECT COUNT(*) FROM task_updates", [], |row| row.get(0))?;
            Ok((tasks, updates))
        })?;

        Ok(RestoreSummary {
            source_path: source_path.display().to_string(),
            snapshot_path: snapshot.display().to_string(),
            task_count,
            update_count,
            schema_version,
        })
    }

    /// 把某个数据库文件的全部内容覆盖到当前在线连接。
    fn copy_from(&self, source: &Path) -> AppResult<()> {
        let src = Connection::open_with_flags(source, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let mut guard = self.lock()?;

        // 覆盖期间先关闭外键检查，完成后统一校验
        guard.execute_batch("PRAGMA foreign_keys = OFF;")?;
        let result = {
            let backup = rusqlite::backup::Backup::new(&src, &mut guard)?;
            backup.run_to_completion(256, Duration::from_millis(5), None)
        };
        let _ = guard.execute_batch("PRAGMA foreign_keys = ON;");
        result?;
        Ok(())
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

/// 恢复前检查备份文件：完整性、schema 版本、是否 ShiftNote 数据库。
fn inspect_backup(path: &Path) -> AppResult<i64> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;

    let check: String = conn.query_row("PRAGMA quick_check(1)", [], |row| row.get(0))?;
    if !check.eq_ignore_ascii_case("ok") {
        return Err(AppError::validation(format!("备份文件已损坏：{check}")));
    }

    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version > migrations::latest_version() {
        return Err(AppError::validation(
            "该备份来自更高版本的 ShiftNote，请先升级应用再恢复",
        ));
    }

    let required: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master
          WHERE type = 'table' AND name IN ('tasks', 'task_updates')",
        [],
        |row| row.get(0),
    )?;
    if required < 2 {
        return Err(AppError::validation("该文件不是有效的 ShiftNote 备份"));
    }

    Ok(version)
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
                let mut stmt = conn.prepare(
                    "SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name",
                )?;
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
        assert!((23..=25).contains(&span_hours), "跨度异常：{span_hours} 小时");
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
        assert_eq!(task.department.as_deref(), Some("张伟"), "按 assignee 回填部门");
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
        assert!(outcome.db.with_conn(|c| Ok(c.query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get::<_, i64>(0))?)).is_ok());

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
        let dir = std::env::temp_dir().join(format!("shiftnote-badrestore-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        let junk = dir.join("junk.db");
        {
            let conn = Connection::open(&junk).unwrap();
            conn.execute_batch("CREATE TABLE foo (id INTEGER);").unwrap();
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
