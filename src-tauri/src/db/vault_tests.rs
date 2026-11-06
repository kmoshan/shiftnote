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

#[test]
fn skipping_setup_persists_plaintext_and_reopens_without_another_prompt() {
    let mut f = Fixture::new();
    f.db().continue_without_password().unwrap();
    add(f.db(), "跳过口令后的记录").unwrap();
    assert!(!crate::vault::is_encrypted(f.db().path()).unwrap());
    assert_eq!(f.db().backup_extension().unwrap(), "db");
    let plain =
        Connection::open_with_flags(f.db().path(), OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(
        plain
            .query_row("SELECT title FROM tasks", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "跳过口令后的记录"
    );
    drop(plain);
    f.reopen();
    let status = f.db().vault_status().unwrap();
    assert!(status.unlocked);
    assert!(!status.configured);
    assert!(!status.legacy);
    assert_eq!(titles(f.db()), ["跳过口令后的记录"]);
    let failed: AppResult<()> = f.db().with_conn(|conn| {
        super::settings::set(conn, "temporary", "discard")?;
        Err(AppError::Other("模拟业务失败".into()))
    });
    assert!(failed.is_err());
    assert_eq!(
        f.db()
            .with_conn(|conn| super::settings::get(conn, "temporary"))
            .unwrap(),
        None
    );
    add(f.db(), "重启后的新增").unwrap();
    f.reopen();
    assert_eq!(titles(f.db()).len(), 2);
}

#[test]
fn settings_can_encrypt_existing_plaintext_without_losing_records() {
    let mut f = Fixture::new();
    f.db().continue_without_password().unwrap();
    add(f.db(), "启用加密前的记录").unwrap();
    let before = std::fs::read(f.db().path()).unwrap();
    assert!(f.db().enable_encryption("short", Some("short")).is_err());
    assert_eq!(std::fs::read(f.db().path()).unwrap(), before);
    f.db().enable_encryption(PASSWORD, Some(PASSWORD)).unwrap();
    assert!(crate::vault::is_encrypted(f.db().path()).unwrap());
    assert_eq!(f.db().backup_extension().unwrap(), "snbak");
    assert!(f.db().vault_status().unwrap().configured);
    let encrypted = std::fs::read(f.db().path()).unwrap();
    f.reopen();
    assert!(!f.db().vault_status().unwrap().unlocked);
    assert!(f.db().continue_without_password().is_err());
    assert_eq!(std::fs::read(f.db().path()).unwrap(), encrypted);
    f.db().unlock_vault(PASSWORD, None, false).unwrap();
    assert_eq!(titles(f.db()), ["启用加密前的记录"]);
}

#[test]
fn plain_mode_backups_and_restore_preserve_the_users_encryption_choice() {
    let mut target = Fixture::new();
    target.db().continue_without_password().unwrap();
    add(target.db(), "当前普通库记录").unwrap();
    let own_backup = target.dir.join("own.db");
    target.db().backup_to(&own_backup).unwrap();
    assert!(!crate::vault::is_encrypted(&own_backup).unwrap());
    let source = Fixture::new();
    source.unlock();
    add(source.db(), "来自加密备份").unwrap();
    let encrypted_backup = source.dir.join("encrypted.snbak");
    source.db().backup_to(&encrypted_backup).unwrap();
    assert!(target
        .db()
        .restore_with_password(&encrypted_backup, None)
        .is_err());
    assert_eq!(titles(target.db()), ["当前普通库记录"]);
    let summary = target
        .db()
        .restore_with_password(&encrypted_backup, Some(PASSWORD))
        .unwrap();
    assert!(summary.snapshot_path.ends_with(".db"));
    assert!(!crate::vault::is_encrypted(Path::new(&summary.snapshot_path)).unwrap());
    target.reopen();
    assert!(target.db().vault_status().unwrap().unlocked);
    assert!(!target.db().vault_status().unwrap().configured);
    assert_eq!(titles(target.db()), ["来自加密备份"]);
    target.db().restore_from(&own_backup).unwrap();
    assert_eq!(titles(target.db()), ["当前普通库记录"]);
    assert!(target.db().encrypt_legacy_backups().is_err());
}

#[test]
fn skipping_legacy_setup_preserves_existing_data_and_migrates_once() {
    let mut f = Fixture::new();
    let legacy = Db::open_or_recover(f.db().path()).unwrap().db;
    add(&legacy, "旧版已有记录").unwrap();
    drop(legacy);
    f.db().continue_without_password().unwrap();
    assert_eq!(titles(f.db()), ["旧版已有记录"]);
    f.reopen();
    assert!(f.db().vault_status().unwrap().unlocked);
    assert!(!f.db().vault_status().unwrap().legacy);
    assert_eq!(titles(f.db()), ["旧版已有记录"]);
}

#[cfg(windows)]
#[test]
fn encryption_publish_failure_keeps_plain_mode_and_allows_retry() {
    use std::os::windows::fs::OpenOptionsExt;
    let mut f = Fixture::new();
    f.db().continue_without_password().unwrap();
    add(f.db(), "加密失败不能丢失记录").unwrap();
    let before = std::fs::read(f.db().path()).unwrap();
    let reader = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(f.db().path())
        .unwrap();
    assert!(f.db().enable_encryption(PASSWORD, Some(PASSWORD)).is_err());
    assert_eq!(std::fs::read(f.db().path()).unwrap(), before);
    assert!(f.db().vault_status().unwrap().unlocked);
    assert!(!f.db().vault_status().unwrap().configured);
    drop(reader);
    f.db().enable_encryption(PASSWORD, Some(PASSWORD)).unwrap();
    f.reopen();
    f.db().unlock_vault(PASSWORD, None, false).unwrap();
    assert_eq!(titles(f.db()), ["加密失败不能丢失记录"]);
}

fn backup_business_rows(db: &Db) -> Vec<Vec<Vec<rusqlite::types::Value>>> {
    db.with_conn(|conn| {
        let mut tables = Vec::new();
        for table in [
            "categories",
            "tasks",
            "task_updates",
            "reminders",
            "handovers",
            "handover_tasks",
            "settings",
        ] {
            // 首次设置标记由目标会话维护，比较其余全部业务字段（包括快照 JSON）。
            let sql = if table == "settings" {
                "SELECT key, value FROM settings WHERE key <> 'security.setup_complete'".to_string()
            } else {
                format!("SELECT * FROM {table}")
            };
            let mut statement = conn.prepare(&sql)?;
            let columns = statement.column_count();
            let mut rows = statement
                .query_map([], |row| {
                    (0..columns)
                        .map(|index| row.get(index))
                        .collect::<rusqlite::Result<Vec<rusqlite::types::Value>>>()
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows.sort_by_cached_key(|row| format!("{row:?}"));
            tables.push(rows);
        }
        Ok(tables)
    })
    .unwrap()
}

#[test]
fn complete_backup_roundtrip_preserves_all_tables_snapshots_and_exports_in_all_modes() {
    use crate::models::{CreateTaskInput, HandoverItemInput, HandoverSaveInput, TaskPatch};

    for source_encrypted in [false, true] {
        let source = Fixture::new();
        if source_encrypted {
            source.unlock();
        } else {
            source.db().continue_without_password().unwrap();
        }
        let finalized = source
            .db()
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO categories VALUES ('backup-category', '中文分类', '#112233', 7, 1, 1)",
                    [],
                )?;
                let task = tasks::create(
                    conn,
                    &CreateTaskInput {
                        title: "备份中文事项 *冻结标题*".into(),
                        description: Some("说明首行\n说明次行".into()),
                        department: Some("运维一部".into()),
                        category_id: Some("backup-category".into()),
                        recorded_at: Some(1000),
                        priority: Some(0),
                    },
                )?;
                conn.execute(
                    "INSERT INTO reminders (task_id, enabled, interval_minutes, next_remind_at, created_at, updated_at)
                     VALUES (?, 1, 120, 3000, 1000, 1000)",
                    [&task.id],
                )?;
                let progress = updates::add(conn, &task.id, "进展原文\n第二行", Some("值班甲"))?;
                let draft = handovers::save(
                    conn,
                    &HandoverSaveInput {
                        id: None,
                        title: "备份验证夜班".into(),
                        period_start: progress.created_at - 1000,
                        period_end: progress.created_at + 1000,
                        shift_from: Some("甲".into()),
                        shift_to: Some("乙".into()),
                        note: Some("交接总备注".into()),
                        items: vec![HandoverItemInput {
                            task_id: task.id.clone(),
                            section: "focus".into(),
                            note: Some("逐条备注".into()),
                            selected_update_ids: vec![progress.id.clone()],
                        }],
                    },
                )?;
                let finalized = handovers::finalize(conn, &draft.handover.id)?;
                handovers::duplicate(conn, &finalized.handover.id)?;
                tasks::update(
                    conn,
                    &task.id,
                    &TaskPatch {
                        title: Some("定稿后修改的实时标题".into()),
                        ..Default::default()
                    },
                )?;
                updates::edit(conn, &progress.id, "定稿后修改的实时进展")?;
                tasks::set_completed(conn, &task.id, true)?;
                settings::set(conn, "theme", "dark")?;
                settings::set(conn, "last_author", "恢复后的处理人")?;
                settings::set(conn, "update.lastCheckAt", "123456")?;
                Ok(finalized)
            })
            .unwrap();
        let expected = backup_business_rows(source.db());
        assert!(expected.iter().all(|rows| !rows.is_empty()));
        let backup = source.dir.join(format!(
            "中文 空格备份.{}",
            source.db().backup_extension().unwrap()
        ));
        source.db().backup_to(&backup).unwrap();
        let original_backup = std::fs::read(&backup).unwrap();
        assert_eq!(
            crate::vault::is_encrypted(&backup).unwrap(),
            source_encrypted
        );

        for target_encrypted in [false, true] {
            let mut target = Fixture::new();
            if target_encrypted {
                target
                    .db()
                    .unlock_vault(OTHER_PASSWORD, Some(OTHER_PASSWORD), false)
                    .unwrap();
            } else {
                target.db().continue_without_password().unwrap();
            }
            add(target.db(), "覆盖前应保留在快照的记录").unwrap();
            let before = backup_business_rows(target.db());
            let summary = target
                .db()
                .restore_with_password(&backup, source_encrypted.then_some(PASSWORD))
                .unwrap();
            assert_eq!(summary.schema_version, 3);
            assert_eq!(summary.task_count, 1);
            assert_eq!(summary.update_count, expected[2].len() as i64);
            assert_eq!(backup_business_rows(target.db()), expected);
            assert_eq!(
                crate::vault::is_encrypted(Path::new(&summary.snapshot_path)).unwrap(),
                target_encrypted
            );
            assert_eq!(std::fs::read(&backup).unwrap(), original_backup);
            target.reopen();
            if target_encrypted {
                assert!(!target.db().vault_status().unwrap().unlocked);
                target
                    .db()
                    .unlock_vault(OTHER_PASSWORD, None, false)
                    .unwrap();
            }
            assert_eq!(
                target.db().vault_status().unwrap().configured,
                target_encrypted
            );
            assert!(target.db().vault_status().unwrap().unlocked);
            assert_eq!(backup_business_rows(target.db()), expected);
            let restored = target
                .db()
                .with_conn(|conn| handovers::get(conn, &finalized.handover.id))
                .unwrap();
            for (format, extension) in [("markdown", "md"), ("txt", "txt")] {
                let expected_text = crate::export::render(&finalized, format).unwrap();
                let path = target.dir.join(format!("恢复后 中文交接.{extension}"));
                crate::export::export(&restored, format, &path, false).unwrap();
                assert_eq!(std::fs::read(&path).unwrap(), expected_text.as_bytes());
                assert!(expected_text.contains("进展原文"));
                assert!(!expected_text.contains("定稿后修改"));
            }
            target
                .db()
                .restore_from(Path::new(&summary.snapshot_path))
                .unwrap();
            assert_eq!(backup_business_rows(target.db()), before);
            add(target.db(), "恢复和回退后仍可写入").unwrap();
        }
    }
}

#[test]
fn backup_never_overwrites_an_existing_file_in_plain_or_encrypted_mode() {
    for encrypted in [false, true] {
        let f = Fixture::new();
        if encrypted {
            f.unlock();
        } else {
            f.db().continue_without_password().unwrap();
        }
        add(f.db(), "已有事项").unwrap();
        let path = f.dir.join("不可覆盖的文件.db");
        std::fs::write(&path, b"existing file content").unwrap();
        assert!(f.db().backup_to(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"existing file content");
        assert_eq!(titles(f.db()), ["已有事项"]);
        assert_eq!(std::fs::read_dir(&f.dir).unwrap().count(), 3);
    }
}

#[cfg(windows)]
#[test]
fn restore_publish_failure_preserves_current_data_and_retries_in_both_modes() {
    use std::os::windows::fs::OpenOptionsExt;

    let source = Fixture::new();
    source.db().continue_without_password().unwrap();
    add(source.db(), "可成功恢复的事项").unwrap();
    let backup = source.dir.join("备份.db");
    source.db().backup_to(&backup).unwrap();
    let source_bytes = std::fs::read(&backup).unwrap();
    for encrypted in [false, true] {
        let target = Fixture::new();
        if encrypted {
            target.unlock();
        } else {
            target.db().continue_without_password().unwrap();
        }
        add(target.db(), "恢复失败时应保留").unwrap();
        let before = std::fs::read(target.db().path()).unwrap();
        let reader = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(target.db().path())
            .unwrap();
        assert!(target.db().restore_from(&backup).is_err());
        assert_eq!(std::fs::read(target.db().path()).unwrap(), before);
        assert_eq!(titles(target.db()), ["恢复失败时应保留"]);
        assert!(target.db().vault_status().unwrap().unlocked);
        assert_eq!(target.db().vault_status().unwrap().configured, encrypted);
        assert_eq!(std::fs::read(&backup).unwrap(), source_bytes);
        let snapshots = std::fs::read_dir(target.db().backups_dir())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(
            crate::vault::is_encrypted(&snapshots[0]).unwrap(),
            encrypted
        );
        drop(reader);
        target.db().restore_from(&backup).unwrap();
        assert_eq!(titles(target.db()), ["可成功恢复的事项"]);
        add(target.db(), "文件释放后继续使用").unwrap();
    }
}
