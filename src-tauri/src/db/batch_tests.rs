use super::*;
use crate::db::{tasks, Db};

fn add(conn: &Connection, id: &str) {
    conn.execute(
        "INSERT INTO tasks (id,title,created_at,updated_at) VALUES (?, ?, 1, 1)",
        params![id, id],
    )
    .unwrap();
    conn.execute("INSERT INTO reminders (task_id,enabled,next_remind_at,created_at,updated_at) VALUES (?,1,99,1,1)", [id]).unwrap();
}

fn input(ids: &[&str], action: &str) -> TaskBatchInput {
    TaskBatchInput {
        ids: ids.iter().map(|s| (*s).into()).collect(),
        action: action.into(),
        priority: None,
    }
}

#[test]
fn batch_deduplicates_and_completion_is_idempotent_and_stops_reminders() {
    let db = Db::open_in_memory().unwrap();
    db.with_conn(|conn| {
        add(conn, "a");
        add(conn, "b");
        let result = execute(conn, &input(&["a", "b", "a"], "complete"))?;
        assert_eq!(
            (result.requested, result.changed, result.unchanged),
            (2, 2, 0)
        );
        let first = tasks::get_required(conn, "a")?;
        conn.execute("UPDATE tasks SET completed_at=123 WHERE id='a'", [])?;
        let result = execute(conn, &input(&["a", "b"], "complete"))?;
        assert_eq!((result.changed, result.unchanged), (0, 2));
        assert_eq!(tasks::get_required(conn, "a")?.completed_at, Some(123));
        assert!(!first.reminder_enabled);
        assert!(first.next_remind_at.is_none());
        execute(conn, &input(&["a", "b"], "reopen"))?;
        assert!(!tasks::get_required(conn, "a")?.reminder_enabled);
        Ok(())
    })
    .unwrap();
}

#[test]
fn batch_missing_id_or_invalid_state_fails_whole_selection() {
    let db = Db::open_in_memory().unwrap();
    db.with_conn(|conn| {
        add(conn, "a");
        add(conn, "b");
        let error = execute(conn, &input(&["a", "missing"], "hide")).unwrap_err();
        assert!(
            matches!(error, AppError::Batch { invalid_ids, .. } if invalid_ids == vec!["missing"])
        );
        assert!(!tasks::get_required(conn, "a")?.hidden);
        tasks::trash(conn, "b")?;
        assert!(execute(conn, &input(&["a", "b"], "complete")).is_err());
        assert_eq!(tasks::get_required(conn, "a")?.status, "open");
        assert!(execute(conn, &input(&["a"], "archive")).is_err());
        assert!(execute(conn, &input(&["a"], "deletePermanent")).is_err());
        Ok(())
    })
    .unwrap();
}

#[test]
fn batch_database_failure_rolls_back_prior_tasks_and_reminders() {
    let db = Db::open_in_memory().unwrap();
    db.with_conn(|conn| {
        add(conn, "a"); add(conn, "b");
        conn.execute_batch("CREATE TRIGGER fail_b BEFORE UPDATE ON tasks WHEN NEW.id='b' BEGIN SELECT RAISE(ABORT, 'injected failure'); END;")?;
        assert!(execute(conn, &input(&["a", "b"], "complete")).is_err());
        let a = tasks::get_required(conn, "a")?;
        assert_eq!(a.status, "open"); assert!(a.reminder_enabled); assert_eq!(a.next_remind_at, Some(99));
        Ok(())
    }).unwrap();
}

#[test]
fn batch_checks_parameters_and_unique_limit() {
    let db = Db::open_in_memory().unwrap();
    db.with_conn(|conn| {
        add(conn, "a");
        for value in [
            input(&[], "complete"),
            input(&["a"], "toggle"),
            input(&["a"], "priority"),
            TaskBatchInput {
                priority: Some(4),
                ..input(&["a"], "priority")
            },
            TaskBatchInput {
                ids: (0..1001).map(|i| i.to_string()).collect(),
                ..input(&[], "complete")
            },
        ] {
            assert!(matches!(execute(conn, &value), Err(AppError::Batch { .. })));
        }
        let repeated = TaskBatchInput {
            ids: vec!["a".into(); 1001],
            ..input(&[], "hide")
        };
        assert_eq!(execute(conn, &repeated)?.requested, 1);
        Ok(())
    })
    .unwrap();
}

#[test]
fn batch_archive_reopen_trash_restore_and_delete_apply_explicit_states() {
    let db = Db::open_in_memory().unwrap();
    db.with_conn(|conn| {
        add(conn, "a");
        execute(conn, &input(&["a"], "complete"))?;
        execute(conn, &input(&["a"], "archive"))?;
        assert!(tasks::get_required(conn, "a")?.archived);
        execute(conn, &input(&["a"], "reopen"))?;
        assert!(!tasks::get_required(conn, "a")?.archived);
        execute(
            conn,
            &TaskBatchInput {
                priority: Some(0),
                ..input(&["a"], "priority")
            },
        )?;
        assert_eq!(tasks::get_required(conn, "a")?.priority, 0);
        execute(conn, &input(&["a"], "hide"))?;
        execute(conn, &input(&["a"], "trash"))?;
        execute(conn, &input(&["a"], "restore"))?;
        let restored = tasks::get_required(conn, "a")?;
        assert!(!restored.hidden);
        assert!(!restored.reminder_enabled);
        execute(conn, &input(&["a"], "trash"))?;
        execute(conn, &input(&["a"], "deletePermanent"))?;
        assert!(tasks::get(conn, "a")?.is_none());
        Ok(())
    })
    .unwrap();
}
