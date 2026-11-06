use super::*;
use crate::db::{tasks, updates};

fn conn() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    conn.execute_batch(include_str!("../../migrations/001_init.sql"))
        .unwrap();
    conn.execute_batch(include_str!(
        "../../migrations/002_department_and_recorded_at.sql"
    ))
    .unwrap();
    conn.execute_batch(include_str!("../../migrations/003_handover_snapshots.sql"))
        .unwrap();
    conn
}

fn task(conn: &Connection, title: &str, recorded_at: i64) -> Task {
    tasks::create(
        conn,
        &CreateTaskInput {
            title: title.into(),
            recorded_at: Some(recorded_at),
            ..Default::default()
        },
    )
    .unwrap()
}

fn input(task: &Task) -> HandoverSaveInput {
    HandoverSaveInput {
        id: None,
        title: "夜班交接".into(),
        period_start: 1000,
        period_end: 3000,
        shift_from: Some("甲班".into()),
        shift_to: Some("乙班".into()),
        note: Some("关注网络".into()),
        items: vec![HandoverItemInput {
            task_id: task.id.clone(),
            section: "pending".into(),
            note: Some("跟进".into()),
            selected_update_ids: vec![],
        }],
    }
}

#[test]
fn v1_and_v2_upgrade_preserve_legacy_association_and_delete_sets_null() {
    for starting_version in [1, 2] {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn.execute_batch(include_str!("../../migrations/001_init.sql"))
            .unwrap();
        conn.execute_batch("INSERT INTO tasks(id,title,created_at,updated_at) VALUES('t','旧事项',1,1);
            INSERT INTO handovers(id,handover_date,created_at,updated_at) VALUES('h','2026-11-06',1,1);
            INSERT INTO handover_tasks(handover_id,task_id,section,note,sort_order) VALUES('h','t','focus','旧备注',20);").unwrap();
        if starting_version == 2 {
            conn.execute_batch(include_str!(
                "../../migrations/002_department_and_recorded_at.sql"
            ))
            .unwrap();
        }
        if starting_version == 1 {
            conn.execute_batch(include_str!(
                "../../migrations/002_department_and_recorded_at.sql"
            ))
            .unwrap();
        }
        conn.execute_batch(include_str!("../../migrations/003_handover_snapshots.sql"))
            .unwrap();
        let old = get(&conn, "h").unwrap();
        assert_eq!(old.handover.title, "2026-11-06 交接记录");
        assert_eq!(
            old.handover.period_end - old.handover.period_start,
            86_400_000
        );
        assert_eq!(old.items[0].note.as_deref(), Some("旧备注"));
        conn.execute("DELETE FROM tasks WHERE id='t'", []).unwrap();
        let (count, task_id): (i64, Option<String>) = conn
            .query_row(
                "SELECT COUNT(*), task_id FROM handover_tasks WHERE handover_id='h'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(task_id, None);
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r
                .get::<_, i64>(
                0
            ))
            .unwrap(),
            0
        );
    }
}

#[test]
fn save_reopens_draft_live_and_empty_selection_means_no_updates() {
    let conn = conn();
    let t = task(&conn, "原标题", 1500);
    updates::add(&conn, &t.id, "不应自动选入", Some("小王")).unwrap();
    let draft = save(&conn, &input(&t)).unwrap();
    assert!(draft.items[0].snapshot.updates.is_empty());
    conn.execute("UPDATE tasks SET title='最新标题' WHERE id=?", [&t.id])
        .unwrap();
    assert_eq!(
        get(&conn, &draft.handover.id).unwrap().items[0]
            .snapshot
            .task
            .title,
        "最新标题"
    );
}

#[test]
fn selected_progress_is_validated_and_ordered_by_time() {
    let conn = conn();
    let t = task(&conn, "跨午夜事项", 1500);
    let a = updates::add(&conn, &t.id, "早", None).unwrap();
    let b = updates::add(&conn, &t.id, "晚", None).unwrap();
    conn.execute(
        "UPDATE task_updates SET created_at=1000 WHERE id=?",
        [&a.id],
    )
    .unwrap();
    conn.execute(
        "UPDATE task_updates SET created_at=2999 WHERE id=?",
        [&b.id],
    )
    .unwrap();
    let mut i = input(&t);
    i.items[0].selected_update_ids = vec![b.id.clone(), a.id.clone(), a.id.clone()];
    let saved = save(&conn, &i).unwrap();
    assert_eq!(
        saved.items[0]
            .snapshot
            .updates
            .iter()
            .map(|u| &u.id)
            .collect::<Vec<_>>(),
        vec![&a.id, &b.id]
    );
    conn.execute(
        "UPDATE task_updates SET created_at=3000 WHERE id=?",
        [&b.id],
    )
    .unwrap();
    assert!(save(&conn, &i).is_err(), "结束时间是排除边界");
    i.items[0].selected_update_ids = vec!["not-owned".into()];
    assert!(save(&conn, &i).is_err());
}

#[test]
fn invalid_or_failed_save_does_not_replace_existing_draft() {
    let conn = conn();
    let t = task(&conn, "已有事项", 1500);
    let saved = save(&conn, &input(&t)).unwrap();
    let mut i = input(&t);
    i.id = Some(saved.handover.id.clone());
    i.title = "不得写入".into();
    i.items.push(HandoverItemInput {
        task_id: "missing".into(),
        section: "new".into(),
        note: None,
        selected_update_ids: vec![],
    });
    assert!(save(&conn, &i).is_err());
    assert_eq!(
        get(&conn, &saved.handover.id).unwrap().handover.title,
        "夜班交接"
    );
    i.items.pop();
    conn.execute_batch("CREATE TRIGGER fail_item BEFORE INSERT ON handover_tasks BEGIN SELECT RAISE(ABORT,'simulated insert failure'); END;").unwrap();
    assert!(save(&conn, &i).is_err());
    let unchanged = get(&conn, &saved.handover.id).unwrap();
    assert_eq!(unchanged.handover.title, "夜班交接");
    assert_eq!(unchanged.items.len(), 1);
}

#[test]
fn finalize_freezes_updates_and_task_even_after_permanent_deletion() {
    let conn = conn();
    let t = task(&conn, "中文 *标题*", 1500);
    let u = updates::add(&conn, &t.id, "保留的\n进展", Some("张三")).unwrap();
    conn.execute(
        "UPDATE task_updates SET created_at=2000 WHERE id=?",
        [&u.id],
    )
    .unwrap();
    let mut i = input(&t);
    i.items[0].selected_update_ids = vec![u.id.clone()];
    let saved = save(&conn, &i).unwrap();
    let frozen = finalize(&conn, &saved.handover.id).unwrap();
    let before = crate::export::render(&frozen, "markdown").unwrap();
    conn.execute("UPDATE tasks SET title='被修改' WHERE id=?", [&t.id])
        .unwrap();
    updates::edit(&conn, &u.id, "被编辑").unwrap();
    conn.execute("DELETE FROM tasks WHERE id=?", [&t.id])
        .unwrap();
    let after = get(&conn, &saved.handover.id).unwrap();
    assert_eq!(after.items.len(), 1);
    assert_eq!(after.items[0].task_id, None);
    assert_eq!(after.items[0].snapshot.updates[0].content, "保留的\n进展");
    assert_eq!(crate::export::render(&after, "markdown").unwrap(), before);
    assert_eq!(
        finalize(&conn, &saved.handover.id)
            .unwrap()
            .handover
            .finalized_at,
        frozen.handover.finalized_at
    );
    i.id = Some(saved.handover.id.clone());
    assert!(save(&conn, &i).is_err());
    assert!(delete(&conn, &saved.handover.id).is_err());
    assert!(
        duplicate(&conn, &saved.handover.id).is_err(),
        "不存在实时事项时必须明确拒绝，不能静默丢项"
    );
    assert_eq!(list(&conn, &HandoverFilter::default()).unwrap().total, 1);
}

#[test]
fn failed_finalize_rolls_back_all_snapshots_and_status() {
    let conn = conn();
    let t = task(&conn, "事项", 1500);
    let saved = save(&conn, &input(&t)).unwrap();
    let previous: String = conn
        .query_row("SELECT snapshot_json FROM handover_tasks", [], |r| r.get(0))
        .unwrap();
    conn.execute(
        "UPDATE tasks SET title='定稿前的新内容' WHERE id=?",
        [&t.id],
    )
    .unwrap();
    conn.execute_batch("CREATE TRIGGER fail_finalize BEFORE UPDATE OF status ON handovers WHEN NEW.status='finalized' BEGIN SELECT RAISE(ABORT,'simulated failure'); END;").unwrap();
    assert!(finalize(&conn, &saved.handover.id).is_err());
    assert_eq!(
        get(&conn, &saved.handover.id).unwrap().handover.status,
        "draft"
    );
    assert_eq!(
        conn.query_row("SELECT snapshot_json FROM handover_tasks", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        previous,
        "失败的冻结写入必须回滚到草稿保存内容"
    );
}

#[test]
fn stale_draft_progress_can_be_reopened_but_cannot_be_silently_finalized() {
    let conn = conn();
    let t = task(&conn, "可纠正草稿", 1500);
    let update = updates::add(&conn, &t.id, "后来删除的进展", None).unwrap();
    conn.execute(
        "UPDATE task_updates SET created_at=2000 WHERE id=?",
        [&update.id],
    )
    .unwrap();
    let mut i = input(&t);
    i.items[0].selected_update_ids = vec![update.id.clone()];
    let saved = save(&conn, &i).unwrap();
    conn.execute("DELETE FROM task_updates WHERE id=?", [&update.id])
        .unwrap();
    let reopened = get(&conn, &saved.handover.id).unwrap();
    assert_eq!(reopened.items[0].selected_update_ids, vec![update.id]);
    assert!(reopened.items[0].snapshot.updates.is_empty());
    assert!(finalize(&conn, &saved.handover.id).is_err());
    i.id = Some(saved.handover.id.clone());
    assert!(save(&conn, &i).is_err());
    i.items[0].selected_update_ids.clear();
    save(&conn, &i).unwrap();
    assert!(finalize(&conn, &saved.handover.id).is_ok());
}

#[test]
fn deleted_draft_task_preserves_last_saved_content_and_requires_explicit_removal() {
    let conn = conn();
    let t = task(&conn, "删除前的已保存标题", 1500);
    let saved = save(&conn, &input(&t)).unwrap();
    conn.execute("DELETE FROM tasks WHERE id=?", [&t.id])
        .unwrap();
    let reopened = get(&conn, &saved.handover.id).unwrap();
    assert_eq!(reopened.items.len(), 1);
    assert!(reopened.items[0].task_id.is_none());
    assert_eq!(reopened.items[0].snapshot.task.title, "删除前的已保存标题");
    assert!(finalize(&conn, &saved.handover.id).is_err());
    assert!(duplicate(&conn, &saved.handover.id).is_err());
    let mut corrected = input(&t);
    corrected.id = Some(saved.handover.id.clone());
    corrected.items.clear();
    save(&conn, &corrected).unwrap();
    assert!(get(&conn, &saved.handover.id).unwrap().items.is_empty());
}

#[test]
fn duplicate_is_new_editable_draft_and_delete_only_removes_that_draft() {
    let conn = conn();
    let t = task(&conn, "待交接", 1500);
    let saved = save(&conn, &input(&t)).unwrap();
    finalize(&conn, &saved.handover.id).unwrap();
    let copy = duplicate(&conn, &saved.handover.id).unwrap();
    assert_ne!(copy.handover.id, saved.handover.id);
    assert_eq!(copy.handover.status, "draft");
    assert_eq!(copy.items[0].task_id.as_deref(), Some(t.id.as_str()));
    assert_eq!(copy.handover.finalized_at, None);
    delete(&conn, &copy.handover.id).unwrap();
    assert!(get(&conn, &copy.handover.id).is_err());
    assert!(get(&conn, &saved.handover.id).is_ok());
}

#[test]
fn candidates_use_half_open_period_and_exclude_hidden_archived_and_trash() {
    let conn = conn();
    let start = task(&conn, "区间开始", 1000);
    task(&conn, "区间结束", 3000);
    let done = task(&conn, "已处理", 900);
    conn.execute(
        "UPDATE tasks SET status='done',completed_at=2999 WHERE id=?",
        [&done.id],
    )
    .unwrap();
    for (name, field) in [
        ("隐藏", "hidden=1"),
        ("归档", "archived=1"),
        ("删除", "deleted_at=2000"),
    ] {
        let t = task(&conn, name, 1500);
        conn.execute(&format!("UPDATE tasks SET {field} WHERE id=?"), [&t.id])
            .unwrap();
    }
    let u = updates::add(&conn, &done.id, "已处理进展", None).unwrap();
    conn.execute(
        "UPDATE task_updates SET created_at=1000 WHERE id=?",
        [&u.id],
    )
    .unwrap();
    let mut filter = HandoverCandidateFilter {
        period_start: 1000,
        period_end: 3000,
        kind: "new".into(),
        search: None,
        limit: None,
        offset: None,
    };
    assert_eq!(candidates(&conn, &filter).unwrap().items[0].id, start.id);
    assert_eq!(candidates(&conn, &filter).unwrap().total, 1);
    for kind in ["handled", "progress"] {
        filter.kind = kind.into();
        let page = candidates(&conn, &filter).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].id, done.id);
    }
    filter.kind = "pending".into();
    assert_eq!(candidates(&conn, &filter).unwrap().total, 2);
    filter.period_end = 999;
    assert!(candidates(&conn, &filter).is_err());
}

#[test]
fn candidates_over_500_have_complete_stable_independent_pagination() {
    let conn = conn();
    let tx = conn.unchecked_transaction().unwrap();
    for n in 0..1001 {
        tx.execute("INSERT INTO tasks(id,title,recorded_at,created_at,updated_at) VALUES(?,?,1500,1500,1500)", params![format!("t-{n:04}"), format!("事项{n}")]).unwrap();
    }
    tx.commit().unwrap();
    let mut ids = Vec::new();
    for offset in [0, 400, 800] {
        let p = candidates(
            &conn,
            &HandoverCandidateFilter {
                period_start: 1000,
                period_end: 3000,
                kind: "all".into(),
                search: None,
                limit: Some(400),
                offset: Some(offset),
            },
        )
        .unwrap();
        assert_eq!(p.total, 1001);
        ids.extend(p.items.into_iter().map(|t| t.id));
    }
    assert_eq!(ids.len(), 1001);
    assert_eq!(
        ids.iter().collect::<std::collections::HashSet<_>>().len(),
        1001
    );
    assert!(ids.windows(2).all(|w| w[0] < w[1]));
}

#[test]
fn list_filters_counts_and_paginates_without_literal_wildcards() {
    let conn = conn();
    let t = task(&conn, "事项", 1500);
    let mut i = input(&t);
    i.title = "100%_交接".into();
    let a = save(&conn, &i).unwrap();
    finalize(&conn, &a.handover.id).unwrap();
    i.title = "其他".into();
    save(&conn, &i).unwrap();
    let p = list(
        &conn,
        &HandoverFilter {
            search: Some("%_".into()),
            status: Some("finalized".into()),
            limit: Some(1),
            offset: Some(0),
        },
    )
    .unwrap();
    assert_eq!(p.total, 1);
    assert_eq!(p.items[0].id, a.handover.id);
    assert_eq!(
        list(
            &conn,
            &HandoverFilter {
                limit: Some(1),
                offset: Some(1),
                ..Default::default()
            }
        )
        .unwrap()
        .items
        .len(),
        1
    );
}
