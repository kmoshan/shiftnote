use super::*;

#[test]
fn restore_rejects_incompatible_field_types_beyond_first_page() {
    let f = Fixture::new();
    let source = f.source();
    let conn = Connection::open(&source).unwrap();
    for index in 0..60 {
        conn.execute(
            "INSERT INTO tasks(id,title,created_at,updated_at) VALUES(?1,'类型兼容记录',1,1)",
            [format!("typed-{index}")],
        )
        .unwrap();
    }
    conn.execute(
        "UPDATE tasks SET description=?1 WHERE id='typed-59'",
        [vec![1_u8, 2, 3]],
    )
    .unwrap();
    drop(conn);
    assert!(f.db.restore_from(&source).is_err());
    f.assert_intact();
}

struct Fixture {
    dir: PathBuf,
    db: Db,
}

impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "shiftnote-restore-regression-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Db::open_or_recover(&dir.join("live.db")).unwrap().db;
        db.with_conn(|conn| {
            tasks::create(
                conn,
                &crate::models::CreateTaskInput {
                    title: "必须保留的在线数据".into(),
                    ..Default::default()
                },
            )?;
            Ok(())
        })
        .unwrap();
        Self { dir, db }
    }

    fn source(&self) -> PathBuf {
        let source = self.dir.join(format!("source-{}.db", uuid::Uuid::new_v4()));
        self.db.backup_to(&source).unwrap();
        source
    }

    fn assert_intact(&self) {
        self.db
            .with_conn(|conn| {
                assert_eq!(
                    tasks::list(conn, &Default::default())?[0].title,
                    "必须保留的在线数据"
                );
                let enabled: i64 = conn.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
                assert_eq!(enabled, 1, "失败后必须恢复外键检查");
                Ok(())
            })
            .unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Windows 会在 Db 关闭后才允许删除 live.db；测试目录只含隔离生成数据。
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn restore_rejects_missing_table_without_overwriting_live_data() {
    let f = Fixture::new();
    let source = f.source();
    Connection::open(&source)
        .unwrap()
        .execute_batch("DROP TABLE reminders;")
        .unwrap();
    assert!(
        f.db.restore_from(&source).is_err(),
        "缺少 reminders 的伪完整备份必须拒绝"
    );
    f.assert_intact();
}

#[test]
fn restore_rejects_missing_column_and_false_schema_version() {
    let f = Fixture::new();
    let source = f.source();
    Connection::open(&source)
        .unwrap()
        .execute_batch("ALTER TABLE reminders DROP COLUMN last_notified_at;")
        .unwrap();
    assert!(
        f.db.restore_from(&source).is_err(),
        "当前版本号不能替代完整列检查"
    );
    f.assert_intact();
}

#[test]
fn restore_rejects_dangling_foreign_keys() {
    let f = Fixture::new();
    let source = f.source();
    Connection::open(&source).unwrap().execute_batch("PRAGMA foreign_keys=OFF; INSERT INTO task_updates (id,task_id,content,kind,created_at,updated_at) VALUES ('orphan','missing','孤立进展','progress',1,1);").unwrap();
    assert!(f.db.restore_from(&source).is_err());
    f.assert_intact();
}

#[test]
fn restore_rejects_missing_constraints_even_if_columns_and_version_match() {
    let f = Fixture::new();
    let source = f.source();
    Connection::open(&source).unwrap().execute_batch("PRAGMA foreign_keys=OFF; ALTER TABLE reminders RENAME TO old_reminders; CREATE TABLE reminders AS SELECT * FROM old_reminders; DROP TABLE old_reminders;").unwrap();
    assert!(
        f.db.restore_from(&source).is_err(),
        "伪造相同列但缺少主键和外键的表不能通过预检"
    );
    f.assert_intact();
}

#[test]
fn restore_rejects_unreadable_handover_selection_json() {
    let f = Fixture::new();
    let source = f.source();
    Connection::open(&source).unwrap().execute_batch("INSERT INTO handovers(id,handover_date,created_at,updated_at) VALUES('bad-handover','2026-11-06',1,1); INSERT INTO handover_tasks(id,handover_id,task_id,selected_update_ids_json) SELECT 'bad-item','bad-handover',id,'not json' FROM tasks LIMIT 1;").unwrap();
    assert!(
        f.db.restore_from(&source).is_err(),
        "所有核心交接查询也必须能读取备份，而不只检查事项列表"
    );
    f.assert_intact();
}

#[test]
fn restores_v1_in_staging_without_upgrading_original_backup() {
    let f = Fixture::new();
    let source = f.dir.join("legacy-v1.db");
    let legacy = Connection::open(&source).unwrap();
    legacy
        .execute_batch(include_str!("../../migrations/001_init.sql"))
        .unwrap();
    legacy.execute_batch("PRAGMA user_version=1; INSERT INTO tasks(id,title,assignee,created_at,updated_at) VALUES('legacy','原始v1记录','旧负责部门',123,123);").unwrap();
    drop(legacy);
    let summary = f.db.restore_from(&source).unwrap();
    assert_eq!(summary.schema_version, migrations::latest_version());
    f.db.with_conn(|conn| {
        let task = tasks::get_required(conn, "legacy")?;
        assert_eq!(task.department.as_deref(), Some("旧负责部门"));
        assert_eq!(task.recorded_at, 123);
        Ok(())
    })
    .unwrap();
    let version: i64 = Connection::open(&source)
        .unwrap()
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 1, "源备份保留原始版本，迁移仅发生在临时副本");
}

#[test]
fn restore_rejects_failed_migration_without_touching_source_or_live_data() {
    let f = Fixture::new();
    let source = f.source();
    // 假称 v1，迁移 v2 会因列已经存在而失败。
    Connection::open(&source)
        .unwrap()
        .execute_batch("PRAGMA user_version=1;")
        .unwrap();
    assert!(f.db.restore_from(&source).is_err());
    let version: i64 = Connection::open(&source)
        .unwrap()
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 1, "预检不得修改源备份");
    f.assert_intact();
}

#[test]
fn restore_rolls_back_a_failure_after_online_copy_and_reenables_foreign_keys() {
    let f = Fixture::new();
    let source = f.source();
    Connection::open(&source)
        .unwrap()
        .execute_batch("UPDATE tasks SET title='来源数据';")
        .unwrap();
    let mut attempt = 0;
    let result = f.db.restore_from_using(&source, |src, dest| {
        attempt += 1;
        copy_database(src, dest)?;
        if attempt == 1 {
            Err(AppError::Other("覆盖后注入失败".into()))
        } else {
            Ok(())
        }
    });
    assert!(result.is_err());
    assert_eq!(attempt, 2, "覆盖失败必须执行快照回滚");
    f.assert_intact();
}

#[test]
fn restore_reports_snapshot_path_if_rollback_also_fails() {
    let f = Fixture::new();
    let source = f.source();
    let mut attempt = 0;
    let error =
        f.db.restore_from_using(&source, |src, dest| {
            attempt += 1;
            if attempt == 1 {
                copy_database(src, dest)?;
            }
            dest.execute_batch("PRAGMA foreign_keys=OFF;")?;
            Err(AppError::Other(format!("注入失败 {attempt}")))
        })
        .unwrap_err();
    assert_eq!(attempt, 2);
    let snapshots = std::fs::read_dir(f.db.backups_dir())
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("pre-restore-")
        })
        .collect::<Vec<_>>();
    assert_eq!(snapshots.len(), 1);
    assert!(
        error.to_string().contains(snapshots[0].to_str().unwrap()),
        "回滚失败需报告可人工恢复的快照路径：{error}"
    );
    let foreign_keys: i64 =
        f.db.with_conn(|conn| Ok(conn.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?))
            .unwrap();
    assert_eq!(foreign_keys, 1);
}

#[test]
fn restore_keeps_write_lock_through_copy_failure_and_rollback() {
    let f = Fixture::new();
    let source = f.source();
    let (attempt_tx, attempt_rx) = std::sync::mpsc::channel();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        let mut attempt = 0;
        let result = f.db.restore_from_using(&source, |src, dest| {
            attempt += 1;
            copy_database(src, dest)?;
            if attempt == 1 {
                let db = &f.db;
                let attempt_tx = attempt_tx.clone();
                let done_tx = done_tx.clone();
                scope.spawn(move || {
                    attempt_tx.send(()).unwrap();
                    db.with_conn(|conn| {
                        tasks::create(
                            conn,
                            &crate::models::CreateTaskInput {
                                title: "等待恢复后写入".into(),
                                ..Default::default()
                            },
                        )?;
                        Ok(())
                    })
                    .unwrap();
                    done_tx.send(()).unwrap();
                });
                attempt_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                assert!(
                    done_rx.recv_timeout(Duration::from_millis(50)).is_err(),
                    "恢复期间写入应等待同一连接锁"
                );
                Err(AppError::Other("注入覆盖失败".into()))
            } else {
                assert!(done_rx.try_recv().is_err(), "回滚完成前不能释放写入锁");
                Ok(())
            }
        });
        assert!(result.is_err());
    });
    done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    f.db.with_conn(|conn| {
        assert_eq!(
            tasks::page(conn, &Default::default())?.total,
            2,
            "恢复回滚结束后写入应保留"
        );
        Ok(())
    })
    .unwrap();
}

fn restore_with_limits(
    f: &Fixture,
    source: &Path,
    limits: restore_validation::RestoreLimits,
) -> AppResult<RestoreSummary> {
    f.db.restore_from_with_limits(source, &limits, copy_database)
}

#[test]
fn restore_rejects_additional_schema_objects_without_using_them() {
    // 都是无副作用的空结构。只测试预检拒绝，不执行这些对象，也不构造破坏性操作。
    for definition in [
        "CREATE TABLE harmless_extra(value TEXT);",
        "CREATE VIEW harmless_view AS SELECT 1 AS value;",
        "CREATE TRIGGER harmless_trigger AFTER INSERT ON tasks BEGIN SELECT 1; END;",
        "CREATE VIRTUAL TABLE harmless_search USING fts5(content);",
    ] {
        let f = Fixture::new();
        let source = f.source();
        Connection::open(&source)
            .unwrap()
            .execute_batch(definition)
            .unwrap();
        let error = f.db.restore_from(&source).unwrap_err();
        assert!(matches!(error, AppError::Validation(_)), "{error}");
        assert!(
            !f.db.backups_dir().exists(),
            "结构不符时不应进入覆盖或创建恢复快照"
        );
        f.assert_intact();
    }
}

#[test]
fn restore_checks_constraints_and_full_index_definition_before_legacy_migration() {
    let baseline = include_str!("../../migrations/001_init.sql");
    for altered in [
        baseline.replace(" CHECK (priority BETWEEN 0 AND 3)", ""),
        baseline.replace("tasks (created_at DESC)", "tasks (created_at ASC)"),
        baseline.replace(
            "tasks (priority, sort_order);",
            "tasks (priority, sort_order) WHERE hidden = 0;",
        ),
    ] {
        assert_ne!(altered, baseline);
        let f = Fixture::new();
        let source = f.dir.join("incompatible-v1.db");
        let conn = Connection::open(&source).unwrap();
        conn.execute_batch(&altered).unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
        drop(conn);
        assert!(f.db.restore_from(&source).is_err());
        let conn = Connection::open(&source).unwrap();
        assert_eq!(migrations::current_version(&conn).unwrap(), 1);
        assert!(!f.db.backups_dir().exists());
        f.assert_intact();
    }
}

#[test]
fn restore_accepts_each_supported_schema_and_migrates_only_the_copy() {
    for version in 1..=migrations::latest_version() {
        let f = Fixture::new();
        let source = f.dir.join("supported.db");
        let conn = Connection::open(&source).unwrap();
        for migration in migrations::MIGRATIONS
            .iter()
            .filter(|m| m.version <= version)
        {
            conn.execute_batch(migration.sql).unwrap();
        }
        conn.pragma_update(None, "user_version", version).unwrap();
        conn.execute(
            "INSERT INTO tasks(id,title,created_at,updated_at) VALUES(?1,?2,1,1)",
            ["imported", "正常历史记录"],
        )
        .unwrap();
        drop(conn);
        let summary = f.db.restore_from(&source).unwrap();
        assert_eq!(summary.task_count, 1);
        f.db.with_conn(|conn| {
            assert_eq!(tasks::get_required(conn, "imported")?.title, "正常历史记录");
            tasks::create(
                conn,
                &crate::models::CreateTaskInput {
                    title: "恢复后正常新增".into(),
                    ..Default::default()
                },
            )?;
            Ok(())
        })
        .unwrap();
        assert_eq!(
            migrations::current_version(&Connection::open(&source).unwrap()).unwrap(),
            version
        );
    }
}

#[test]
fn restore_enforces_file_size_boundary_before_changing_live_data() {
    let f = Fixture::new();
    let source = f.source();
    let bytes = std::fs::metadata(&source).unwrap().len();
    assert!(restore_with_limits(
        &f,
        &source,
        restore_validation::RestoreLimits {
            max_file_bytes: bytes,
            ..Default::default()
        }
    )
    .is_ok());
    assert!(restore_with_limits(
        &f,
        &source,
        restore_validation::RestoreLimits {
            max_file_bytes: bytes - 1,
            ..Default::default()
        }
    )
    .is_err());
    f.assert_intact();
}

#[test]
fn restore_enforces_streamed_record_and_field_boundaries() {
    let f = Fixture::new();
    let source = f.source();
    assert!(restore_with_limits(
        &f,
        &source,
        restore_validation::RestoreLimits {
            max_rows_per_table: 1,
            max_total_rows: 1,
            ..Default::default()
        }
    )
    .is_ok());
    for limits in [
        restore_validation::RestoreLimits {
            max_rows_per_table: 0,
            ..Default::default()
        },
        restore_validation::RestoreLimits {
            max_total_rows: 0,
            ..Default::default()
        },
        restore_validation::RestoreLimits {
            max_value_bytes: 1,
            ..Default::default()
        },
    ] {
        assert!(restore_with_limits(&f, &source, limits).is_err());
        f.assert_intact();
    }
}

#[test]
fn restore_bounds_valid_snapshot_bytes_before_deserialization() {
    let f = Fixture::new();
    let source = f.source();
    let conn = Connection::open(&source).unwrap();
    let task = tasks::list(&conn, &Default::default()).unwrap().remove(0);
    let detail = handovers::save(
        &conn,
        &crate::models::HandoverSaveInput {
            id: None,
            title: "正常交接".into(),
            period_start: task.recorded_at - 1,
            period_end: task.recorded_at + 1,
            shift_from: None,
            shift_to: None,
            note: None,
            items: vec![crate::models::HandoverItemInput {
                task_id: task.id,
                section: "pending".into(),
                note: None,
                selected_update_ids: vec![],
            }],
        },
    )
    .unwrap();
    handovers::finalize(&conn, &detail.handover.id).unwrap();
    let bytes: usize = conn
        .query_row(
            "SELECT length(CAST(snapshot_json AS BLOB)) FROM handover_tasks",
            [],
            |row| row.get::<_, i64>(0).map(|value| value as usize),
        )
        .unwrap();
    drop(conn);
    assert!(restore_with_limits(
        &f,
        &source,
        restore_validation::RestoreLimits {
            max_snapshot_bytes: bytes,
            max_total_snapshot_bytes: bytes,
            max_selection_bytes: 2,
            ..Default::default()
        }
    )
    .is_ok());
    for limits in [
        restore_validation::RestoreLimits {
            max_snapshot_bytes: bytes - 1,
            ..Default::default()
        },
        restore_validation::RestoreLimits {
            max_total_snapshot_bytes: bytes - 1,
            ..Default::default()
        },
        restore_validation::RestoreLimits {
            max_selection_bytes: 1,
            ..Default::default()
        },
    ] {
        assert!(restore_with_limits(&f, &source, limits).is_err());
        f.assert_intact();
    }
}

#[test]
fn sqlite_budget_interrupts_ordinary_queries_and_restores_connection_limits() {
    use rusqlite::limits::Limit;
    let db = Db::open_in_memory().unwrap();
    db.with_conn_mut(|conn| {
        let original = conn.limit(Limit::SQLITE_LIMIT_LENGTH)?;
        {
            let limits = restore_validation::RestoreLimits {
                max_vm_steps: 1,
                ..Default::default()
            };
            let budget = restore_validation::SqlBudget::new(&mut *conn, &limits)?;
            assert!(
                tasks::counts(&budget).is_err(),
                "普通业务查询也必须受 VM 预算约束"
            );
        }
        assert_eq!(conn.limit(Limit::SQLITE_LIMIT_LENGTH)?, original);
        tasks::create(
            conn,
            &crate::models::CreateTaskInput {
                title: "预算释放后正常写入".into(),
                ..Default::default()
            },
        )?;
        assert_eq!(tasks::counts(conn)?.all, 1);
        Ok(())
    })
    .unwrap();
}

#[test]
fn restore_budget_failure_preserves_live_data_and_connection_protections() {
    let f = Fixture::new();
    let source = f.source();
    for limits in [
        restore_validation::RestoreLimits {
            max_vm_steps: 0,
            ..Default::default()
        },
        restore_validation::RestoreLimits {
            max_duration: Duration::ZERO,
            ..Default::default()
        },
    ] {
        assert!(restore_with_limits(&f, &source, limits).is_err());
        f.assert_intact();
    }
    f.db.with_conn(|conn| {
        assert!(conn.db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_DEFENSIVE)?);
        let trusted: i64 = conn.query_row("PRAGMA trusted_schema", [], |r| r.get(0))?;
        assert_eq!(trusted, 0);
        tasks::create(
            conn,
            &crate::models::CreateTaskInput {
                title: "保护开启时正常写入".into(),
                ..Default::default()
            },
        )?;
        Ok(())
    })
    .unwrap();
}
