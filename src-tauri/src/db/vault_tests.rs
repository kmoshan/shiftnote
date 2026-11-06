use super::*;

const PASSWORD: &str = "vault-tests-correct-password-123";
const OTHER_PASSWORD: &str = "vault-tests-other-password-456";

struct Fixture {
    dir: PathBuf,
    db: Option<Db>,
}

impl Fixture {
    fn new() -> Self {
        let dir =
            std::env::temp_dir().join(format!("shiftnote-vault-tests-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Db::new_vault(&dir.join("shiftnote.db")).unwrap();
        Self { dir, db: Some(db) }
    }
    fn db(&self) -> &Db {
        self.db.as_ref().unwrap()
    }
    fn unlock(&self) {
        self.db()
            .unlock_vault(PASSWORD, Some(PASSWORD), false)
            .unwrap();
    }
    fn reopen(&mut self) {
        self.db.take();
        self.db = Some(Db::new_vault(&self.dir.join("shiftnote.db")).unwrap());
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.db.take();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn add(db: &Db, title: &str) -> AppResult<crate::models::Task> {
    db.with_conn(|conn| {
        tasks::create(
            conn,
            &crate::models::CreateTaskInput {
                title: title.into(),
                ..Default::default()
            },
        )
    })
}

fn titles(db: &Db) -> Vec<String> {
    db.with_conn(|conn| {
        Ok(tasks::list(conn, &Default::default())?
            .into_iter()
            .map(|task| task.title)
            .collect())
    })
    .unwrap()
}

#[test]
fn vault_is_locked_until_confirmed_and_reopens_with_correct_password() {
    let mut f = Fixture::new();
    assert!(!f.db().vault_status().unwrap().configured);
    assert!(add(f.db(), "未解锁写入").is_err());
    assert!(f
        .db()
        .unlock_vault(PASSWORD, Some("different-confirmation"), false)
        .is_err());
    assert!(!f.db().path.exists());
    f.unlock();
    add(f.db(), "仅存密文的中文事项").unwrap();
    let encrypted = std::fs::read(f.db().path()).unwrap();
    assert!(crate::vault::is_encrypted(f.db().path()).unwrap());
    assert!(!encrypted
        .windows("仅存密文的中文事项".len())
        .any(|part| part == "仅存密文的中文事项".as_bytes()));
    f.reopen();
    assert!(!f.db().vault_status().unwrap().unlocked);
    assert!(f.db().unlock_vault(OTHER_PASSWORD, None, false).is_err());
    assert_eq!(std::fs::read(f.db().path()).unwrap(), encrypted);
    f.db().unlock_vault(PASSWORD, None, false).unwrap();
    assert_eq!(titles(f.db()), ["仅存密文的中文事项"]);
    let (temp, journal): (i64, String) = f
        .db()
        .with_conn(|conn| {
            Ok((
                conn.query_row("PRAGMA temp_store", [], |r| r.get(0))?,
                conn.query_row("PRAGMA journal_mode", [], |r| r.get(0))?,
            ))
        })
        .unwrap();
    assert_eq!(temp, 2);
    assert_eq!(journal, "memory");
    assert!(!sidecar_path(f.db().path(), "-wal").exists());
}

#[test]
fn instance_lock_rejects_second_vault_for_same_path() {
    let f = Fixture::new();
    assert!(Db::new_vault(f.db().path()).is_err());
}

#[test]
fn legacy_conversion_requires_consent_and_preserves_uncheckpointed_wal_records() {
    let f = Fixture::new();
    let path = f.db().path();
    let mut legacy = Connection::open(path).unwrap();
    migrations::apply(&mut legacy).unwrap();
    legacy
        .execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;")
        .unwrap();
    legacy
        .set_db_config(
            rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE,
            true,
        )
        .unwrap();
    tasks::create(
        &legacy,
        &crate::models::CreateTaskInput {
            title: "WAL 中必须保留的记录".into(),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(sidecar_path(path, "-wal").metadata().unwrap().len() > 0);
    drop(legacy);
    assert!(f.db().vault_status().unwrap().legacy);
    assert!(f
        .db()
        .unlock_vault(PASSWORD, Some(PASSWORD), false)
        .is_err());
    assert!(!crate::vault::is_encrypted(path).unwrap());
    f.db().unlock_vault(PASSWORD, Some(PASSWORD), true).unwrap();
    assert!(crate::vault::is_encrypted(path).unwrap());
    assert_eq!(titles(f.db()), ["WAL 中必须保留的记录"]);
    assert!(!sidecar_path(path, "-wal").exists());
    assert!(!sidecar_path(path, "-shm").exists());
    assert!(!f.db().backups_dir().exists(), "主库迁移不额外留下明文副本");
}

#[cfg(windows)]
#[test]
fn legacy_conversion_rejects_a_database_open_in_an_old_instance() {
    let f = Fixture::new();
    let mut legacy = Connection::open(f.db().path()).unwrap();
    migrations::apply(&mut legacy).unwrap();
    assert!(f.db().unlock_vault(PASSWORD, Some(PASSWORD), true).is_err());
    assert!(!crate::vault::is_encrypted(f.db().path()).unwrap());
    assert!(!f.db().vault_status().unwrap().unlocked);
    drop(legacy);
    f.db().unlock_vault(PASSWORD, Some(PASSWORD), true).unwrap();
}

#[test]
fn malformed_legacy_schema_is_rejected_before_encryption() {
    let f = Fixture::new();
    let mut legacy = Connection::open(f.db().path()).unwrap();
    migrations::apply(&mut legacy).unwrap();
    // 无副作用结构，单测只验证拒绝，不调用视图。
    legacy
        .execute_batch("CREATE VIEW harmless_view AS SELECT 1 AS value;")
        .unwrap();
    drop(legacy);
    let original = std::fs::read(f.db().path()).unwrap();
    assert!(f.db().unlock_vault(PASSWORD, Some(PASSWORD), true).is_err());
    assert_eq!(std::fs::read(f.db().path()).unwrap(), original);
    assert!(!f.db().vault_status().unwrap().unlocked);
}

#[test]
fn secure_restore_accepts_cross_machine_password_and_keeps_encrypted_snapshot() {
    let source = Fixture::new();
    source.unlock();
    add(source.db(), "跨机备份数据").unwrap();
    let backup = source.dir.join("transfer.snbak");
    source.db().backup_to(&backup).unwrap();
    assert!(crate::vault::is_encrypted(&backup).unwrap());
    let destination = Fixture::new();
    destination
        .db()
        .unlock_vault(OTHER_PASSWORD, Some(OTHER_PASSWORD), false)
        .unwrap();
    add(destination.db(), "恢复前数据").unwrap();
    let before = std::fs::read(destination.db().path()).unwrap();
    assert!(destination
        .db()
        .restore_with_password(&backup, Some("incorrect-backup-password"))
        .is_err());
    assert_eq!(std::fs::read(destination.db().path()).unwrap(), before);
    assert_eq!(titles(destination.db()), ["恢复前数据"]);
    let summary = destination
        .db()
        .restore_with_password(&backup, Some(PASSWORD))
        .unwrap();
    assert_eq!(titles(destination.db()), ["跨机备份数据"]);
    assert!(summary.snapshot_path.ends_with(".snbak"));
    assert!(crate::vault::is_encrypted(Path::new(&summary.snapshot_path)).unwrap());
    destination
        .db()
        .restore_from(Path::new(&summary.snapshot_path))
        .unwrap();
    assert_eq!(titles(destination.db()), ["恢复前数据"]);
    assert_eq!(destination.db().plaintext_backup_count().unwrap(), 0);
}

#[test]
fn plaintext_backup_conversion_verifies_ciphertext_and_retains_invalid_files() {
    let f = Fixture::new();
    f.unlock();
    std::fs::create_dir_all(f.db().backups_dir()).unwrap();
    let plain = f.db().backups_dir().join("old-backup.db");
    let legacy = Db::open_or_recover(&plain).unwrap().db;
    add(&legacy, "旧备份记录").unwrap();
    drop(legacy);
    let invalid = f.db().backups_dir().join("invalid.db");
    std::fs::write(&invalid, b"not a SQLite backup").unwrap();
    assert_eq!(f.db().plaintext_backup_count().unwrap(), 2);
    let (converted, remaining) = f.db().encrypt_legacy_backups().unwrap();
    assert_eq!((converted, remaining), (1, 1));
    assert!(!plain.exists());
    assert_eq!(std::fs::read(&invalid).unwrap(), b"not a SQLite backup");
    let encrypted = std::fs::read_dir(f.db().backups_dir())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.extension()
                .is_some_and(|extension| extension == "snbak")
        })
        .unwrap();
    f.db().restore_from(&encrypted).unwrap();
    assert_eq!(titles(f.db()), ["旧备份记录"]);
}

#[cfg(windows)]
#[test]
fn failed_publish_reloads_last_saved_image_and_allows_retry() {
    use std::os::windows::fs::OpenOptionsExt;
    let f = Fixture::new();
    f.unlock();
    add(f.db(), "已保存记录").unwrap();
    let reader = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(f.db().path())
        .unwrap();
    assert!(add(f.db(), "写入失败不应留下").is_err());
    assert_eq!(titles(f.db()), ["已保存记录"]);
    assert!(f.db().vault_status().unwrap().unlocked);
    drop(reader);
    add(f.db(), "重试成功").unwrap();
    assert_eq!(titles(f.db()).len(), 2);
}

#[test]
fn persistence_and_reload_failure_lock_the_session_closed() {
    let mut f = Fixture::new();
    f.unlock();
    add(f.db(), "原文件中的记录").unwrap();
    let original = f.db().path.clone();
    let blocked = f.dir.join("directory-instead-of-file");
    std::fs::create_dir(&blocked).unwrap();
    f.db.as_mut().unwrap().path = blocked;
    assert!(add(f.db(), "不能保存的记录").is_err());
    assert!(!f.db().unlocked.load(Ordering::Acquire));
    assert!(!f.db().vault_status().is_ok_and(|status| status.unlocked));
    assert!(add(f.db(), "锁定后禁止写入").is_err());
    assert!(f.db().vault.lock().unwrap().is_none());
    f.db.take();
    let reopened = Db::new_vault(&original).unwrap();
    reopened.unlock_vault(PASSWORD, None, false).unwrap();
    assert_eq!(titles(&reopened), ["原文件中的记录"]);
}

#[test]
fn failed_operation_discards_committed_partial_writes_in_memory_and_on_reopen() {
    let mut f = Fixture::new();
    f.unlock();
    add(f.db(), "原有数据").unwrap();
    let before = std::fs::read(f.db().path()).unwrap();
    let result: AppResult<()> = f.db().with_conn(|conn| {
        tasks::create(
            conn,
            &crate::models::CreateTaskInput {
                title: "闭包失败不得保留".into(),
                ..Default::default()
            },
        )?;
        Err(AppError::validation("模拟后续业务验证失败"))
    });
    assert!(result.is_err());
    assert_eq!(titles(f.db()), ["原有数据"]);
    assert_eq!(std::fs::read(f.db().path()).unwrap(), before);
    let result: AppResult<()> = f.db().with_conn_mut(|conn| {
        settings::set(conn, "partial", "应当丢弃")?;
        Err(AppError::validation("模拟可变闭包后续失败"))
    });
    assert!(result.is_err());
    assert!(f
        .db()
        .with_conn(|conn| settings::get(conn, "partial"))
        .unwrap()
        .is_none());
    f.reopen();
    f.db().unlock_vault(PASSWORD, None, false).unwrap();
    assert_eq!(titles(f.db()), ["原有数据"]);
}

#[test]
fn writes_over_reopen_limits_are_rejected_without_persisting_them() {
    let mut f = Fixture::new();
    f.unlock();
    add(f.db(), "仍可重开的数据").unwrap();
    let before = std::fs::read(f.db().path()).unwrap();
    let value = "a".repeat(8 * 1024 * 1024 + 1);
    assert!(f
        .db()
        .with_conn(|conn| settings::set(conn, "too-large", &value))
        .is_err());
    assert_eq!(std::fs::read(f.db().path()).unwrap(), before);
    assert!(f
        .db()
        .with_conn(|conn| settings::get(conn, "too-large"))
        .unwrap()
        .is_none());
    f.reopen();
    f.db().unlock_vault(PASSWORD, None, false).unwrap();
    assert_eq!(titles(f.db()), ["仍可重开的数据"]);
}

#[test]
fn legacy_source_changed_after_snapshot_is_rejected_without_replacing_it() {
    use super::vault_storage::{source_fingerprint, SourceLease};
    let f = Fixture::new();
    let path = f.dir.join("changing-legacy.db");
    let source = Db::open_or_recover(&path).unwrap().db;
    add(&source, "第一版记录").unwrap();
    drop(source);
    let expected = source_fingerprint(&mut std::fs::File::open(&path).unwrap()).unwrap();
    // 合法业务写入模拟 own SQLite 关闭与取得租约之间的旧实例更新。
    let source = Db::open_or_recover(&path).unwrap().db;
    add(&source, "间隙中新记录").unwrap();
    drop(source);
    assert!(SourceLease::acquire_verified(&path, &expected).is_err());
    let source = Db::open_or_recover(&path).unwrap().db;
    assert_eq!(titles(&source).len(), 2);
    assert!(!crate::vault::is_encrypted(&path).unwrap());
}

#[cfg(windows)]
#[test]
fn source_lease_blocks_concurrent_write_handles_but_allows_atomic_publish() {
    use super::vault_storage::SourceLease;
    let f = Fixture::new();
    f.unlock();
    add(f.db(), "租约前数据").unwrap();
    let lease = SourceLease::acquire(f.db().path()).unwrap();
    let path = f.db().path().to_owned();
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                assert!(std::fs::OpenOptions::new().write(true).open(&path).is_err());
                assert!(std::fs::OpenOptions::new().read(true).open(&path).is_ok());
            })
            .join()
            .unwrap();
    });
    add(f.db(), "持有租约时合法原子保存").unwrap();
    assert_eq!(titles(f.db()).len(), 2);
    drop(lease);
}
