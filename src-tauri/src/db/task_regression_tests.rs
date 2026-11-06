use super::*;
use crate::db::Db;

fn new_task(conn: &Connection, title: &str) -> Task {
    create(
        conn,
        &CreateTaskInput {
            title: title.into(),
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn repeated_completion_preserves_completion_time() {
    let db = Db::open_in_memory().unwrap();
    db.with_conn(|conn| {
        let task = new_task(conn, "已完成事项");
        set_completed(conn, &task.id, true)?;
        conn.execute(
            "UPDATE tasks SET completed_at = 123, updated_at = 456 WHERE id = ?",
            [&task.id],
        )?;
        let again = set_completed(conn, &task.id, true)?;
        assert_eq!(again.completed_at, Some(123), "重试完成不能改写完成时间");
        assert_eq!(again.updated_at, 456, "幂等操作不能改变业务修改时间");
        Ok(())
    })
    .unwrap();
}

#[test]
fn ordinary_views_exclude_archived_and_reopening_unarchives() {
    let db = Db::open_in_memory().unwrap();
    db.with_conn(|conn| {
        let task = new_task(conn, "归档事项");
        set_completed(conn, &task.id, true)?;
        conn.execute("UPDATE tasks SET archived = 1 WHERE id = ?", [&task.id])?;
        for view in ["all", "today", "done", "hidden", "search"] {
            assert!(
                list(
                    conn,
                    &TaskFilter {
                        view: Some(view.into()),
                        ..Default::default()
                    }
                )?
                .is_empty(),
                "{view} 不应包含归档事项"
            );
        }
        assert_eq!(counts(conn)?.all, 0);
        assert!(!set_completed(conn, &task.id, false)?.archived);
        Ok(())
    })
    .unwrap();
}

#[test]
fn permanent_delete_requires_recycle_bin() {
    let db = Db::open_in_memory().unwrap();
    db.with_conn(|conn| {
        let task = new_task(conn, "仍在使用");
        assert!(delete_permanent(conn, &task.id).is_err());
        assert!(get(conn, &task.id)?.is_some());
        Ok(())
    })
    .unwrap();
}

#[test]
fn rejects_reverse_recorded_date_interval() {
    let db = Db::open_in_memory().unwrap();
    db.with_conn(|conn| {
        assert!(list(
            conn,
            &TaskFilter {
                date_from: Some(200),
                date_to: Some(100),
                ..Default::default()
            }
        )
        .is_err());
        Ok(())
    })
    .unwrap();
}

#[test]
fn pages_visit_1001_tied_rows_once_and_report_full_total() {
    let db = Db::open_in_memory().unwrap();
    db.with_conn(|conn| {
        for i in (0..1001).rev() {
            conn.execute(
                "INSERT INTO tasks (id,title,created_at,updated_at) VALUES (?, '同序事项', 1, 1)",
                [format!("task-{i:04}")],
            )?;
        }
        let mut ids = Vec::new();
        for offset in (0..1100).step_by(100) {
            let result = page(
                conn,
                &TaskFilter {
                    limit: Some(100),
                    offset: Some(offset),
                    ..Default::default()
                },
            )?;
            assert_eq!(result.total, 1001);
            assert_eq!(result.limit, 100);
            assert_eq!(result.offset, offset);
            ids.extend(result.items.into_iter().map(|t| t.id));
        }
        assert_eq!(ids.len(), 1001);
        assert_eq!(
            ids,
            (0..1001)
                .map(|i| format!("task-{i:04}"))
                .collect::<Vec<_>>(),
            "全部排序相等时以唯一 ID 保证分页稳定"
        );
        assert_eq!(page(conn, &Default::default())?.items.len(), 50);
        let clamped = page(
            conn,
            &TaskFilter {
                limit: Some(9000),
                offset: Some(-1),
                ..Default::default()
            },
        )?;
        assert_eq!(
            (clamped.limit, clamped.offset, clamped.items.len()),
            (500, 0, 500)
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn page_total_and_items_share_search_priority_and_half_open_recorded_interval() {
    let db = Db::open_in_memory().unwrap();
    db.with_conn(|conn| {
        // 跨年区间；记录日期与创建时间独立。
        let from = 1_735_603_200_000;
        let to = from + 86_400_000;
        for (id, recorded, priority) in [("start", from, 1), ("last", to - 1, 1), ("end", to, 1), ("before", from - 1, 1), ("priority", from, 2)] {
            conn.execute("INSERT INTO tasks (id,title,priority,recorded_at,created_at,updated_at) VALUES (?, '巡检', ?, ?, 1, 1)", params![id, priority, recorded])?;
            crate::db::updates::add(conn, id, "历史关键词", None)?;
        }
        let result = page(conn, &TaskFilter { search: Some("历史关键词".into()), priority: Some(1), date_from: Some(from), date_to: Some(to), limit: Some(1), ..Default::default() })?;
        assert_eq!(result.total, 2);
        assert_eq!(result.items.len(), 1);
        assert!(result.items.iter().all(|t| t.recorded_at >= from && t.recorded_at < to));
        Ok(())
    }).unwrap();
}

#[test]
fn archive_requires_done_stops_reminders_and_updates_counts() {
    let db = Db::open_in_memory().unwrap();
    db.with_conn(|conn| {
        let task = new_task(conn, "待归档");
        assert!(set_archived(conn, &task.id, true).is_err());
        set_completed(conn, &task.id, true)?;
        conn.execute("INSERT INTO reminders (task_id, enabled, next_remind_at, created_at, updated_at) VALUES (?,1,99,1,1)", [&task.id])?;
        let archived = set_archived(conn, &task.id, true)?;
        assert!(archived.archived);
        assert!(!archived.reminder_enabled);
        assert!(archived.next_remind_at.is_none());
        let counts = counts(conn)?;
        assert_eq!((counts.all, counts.done, counts.archived), (0, 0, 1));
        assert_eq!(page(conn, &TaskFilter { view: Some("archived".into()), ..Default::default() })?.total, 1);
        let restored = set_archived(conn, &task.id, false)?;
        assert!(!restored.archived);
        assert!(!restored.reminder_enabled);
        Ok(())
    }).unwrap();
}

#[test]
fn move_uses_full_scope_and_rejects_foreign_or_nonadjacent_anchors() {
    let db = Db::open_in_memory().unwrap();
    db.with_conn(|conn| {
        for i in 0..105 {
            conn.execute("INSERT INTO tasks (id,title,sort_order,created_at,updated_at) VALUES (?, '手动排序', ?, 1, 1)", params![format!("t-{i:03}"), i * 10])?;
        }
        move_task(conn, &TaskMoveInput { id: "t-104".into(), before_id: Some("t-049".into()), after_id: Some("t-050".into()), view: "all".into() })?;
        let result = page(conn, &TaskFilter { sort: Some("manual".into()), limit: Some(500), ..Default::default() })?;
        let ids: Vec<_> = result.items.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(&ids[49..52], &["t-049", "t-104", "t-050"]);
        assert_eq!(ids[0], "t-000", "第二页移动不能重置第一页排序");
        assert!(move_task(conn, &TaskMoveInput { id: "t-104".into(), before_id: Some("t-000".into()), after_id: Some("t-100".into()), view: "all".into() }).is_err());
        set_hidden(conn, "t-050", true)?;
        assert!(move_task(conn, &TaskMoveInput { id: "t-104".into(), before_id: Some("t-050".into()), after_id: None, view: "all".into() }).is_err());
        Ok(())
    }).unwrap();
}

#[test]
fn legacy_reorder_rejects_partial_scope_instead_of_resetting_one_page() {
    let db = Db::open_in_memory().unwrap();
    db.with_conn(|conn| {
        let a = new_task(conn, "第一页");
        let b = new_task(conn, "第二页");
        assert!(
            reorder(conn, &[b.id.clone()]).is_err(),
            "旧批量顺序接口不能按当前页重编号"
        );
        let all = list(
            conn,
            &TaskFilter {
                sort: Some("manual".into()),
                ..Default::default()
            },
        )?;
        assert_eq!(all[0].id, a.id);
        Ok(())
    })
    .unwrap();
}
