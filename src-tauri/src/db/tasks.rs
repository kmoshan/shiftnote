//! 事项（tasks）数据访问层。
//!
//! 所有 SQL 都集中在这里；命令层只负责参数校验与状态编排。
//!
//! 时间口径：
//! - `recorded_at` 是「记录时间」，可由用户手工指定，列表按它筛选日期；
//! - 旧数据可能没有 `recorded_at`，统一用 `COALESCE(recorded_at, created_at)` 兜底。

use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, Connection, Row};
use uuid::Uuid;

use crate::db::{local_day_bounds, now_ms};
use crate::error::{AppError, AppResult};
use crate::models::*;

const TITLE_MAX_CHARS: usize = 200;
const DESC_MAX_CHARS: usize = 20_000;

#[cfg(test)]
#[path = "task_regression_tests.rs"]
mod regression_tests;

/// 统一的「事项日期」表达式：优先记录时间，缺失时退回创建时间。
const TASK_DATE: &str = "COALESCE(t.recorded_at, t.created_at)";

const SELECT_BASE: &str = r#"
SELECT t.id, t.title, t.description, t.priority, t.status, t.completed_at,
       t.hidden, t.archived, t.deleted_at, t.category_id,
       t.department, COALESCE(t.recorded_at, t.created_at) AS recorded_at,
       t.sort_order, t.created_at, t.updated_at,
       lu.content      AS latest_update_content,
       lu.created_at   AS latest_update_at,
       COALESCE(uc.cnt, 0) AS update_count,
       COALESCE(r.enabled, 0) AS reminder_enabled,
       r.next_remind_at AS next_remind_at
  FROM tasks t
  LEFT JOIN (
      SELECT u.task_id, u.content, u.created_at,
             ROW_NUMBER() OVER (PARTITION BY u.task_id ORDER BY u.created_at DESC, u.rowid DESC) AS rn
        FROM task_updates u
       WHERE u.kind = 'progress'
  ) lu ON lu.task_id = t.id AND lu.rn = 1
  LEFT JOIN (
      SELECT task_id, COUNT(*) AS cnt FROM task_updates WHERE kind = 'progress' GROUP BY task_id
  ) uc ON uc.task_id = t.id
  LEFT JOIN reminders r ON r.task_id = t.id
"#;

fn map_task(row: &Row<'_>) -> rusqlite::Result<Task> {
    Ok(Task {
        id: row.get(0)?,
        title: row.get(1)?,
        description: row.get(2)?,
        priority: row.get(3)?,
        status: row.get(4)?,
        completed_at: row.get(5)?,
        hidden: row.get::<_, i64>(6)? != 0,
        archived: row.get::<_, i64>(7)? != 0,
        deleted_at: row.get(8)?,
        category_id: row.get(9)?,
        department: row.get(10)?,
        recorded_at: row.get(11)?,
        sort_order: row.get(12)?,
        created_at: row.get(13)?,
        updated_at: row.get(14)?,
        latest_update_content: row.get(15)?,
        latest_update_at: row.get(16)?,
        update_count: row.get(17)?,
        reminder_enabled: row.get::<_, i64>(18)? != 0,
        next_remind_at: row.get(19)?,
    })
}

fn escape_like(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn text_or_null(value: &str) -> Value {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Value::Null
    } else {
        Value::Text(trimmed.to_string())
    }
}

fn validate_title(title: &str) -> AppResult<String> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return Err(AppError::validation("事项标题不能为空"));
    }
    if trimmed.chars().count() > TITLE_MAX_CHARS {
        return Err(AppError::validation(format!(
            "事项标题不能超过 {TITLE_MAX_CHARS} 个字符"
        )));
    }
    Ok(trimmed.to_string())
}

fn query_parts(filter: &TaskFilter) -> AppResult<(String, String, Vec<Value>)> {
    let mut clauses: Vec<String> = Vec::new();
    let mut values: Vec<Value> = Vec::new();

    let view = filter.view.as_deref().unwrap_or("all");
    match view {
        "all" => clauses.push("t.deleted_at IS NULL AND t.hidden = 0 AND t.archived = 0".into()),
        "today" => {
            // 今日 = 记录时间落在本地今天
            let (start, end) = local_day_bounds(0);
            clauses.push(format!(
                "t.deleted_at IS NULL AND t.hidden = 0 AND t.archived = 0 AND {TASK_DATE} >= ? AND {TASK_DATE} < ?"
            ));
            values.push(Value::Integer(start));
            values.push(Value::Integer(end));
        }
        "open" => clauses.push(
            "t.deleted_at IS NULL AND t.hidden = 0 AND t.archived = 0 AND t.status = 'open'".into(),
        ),
        "focus" => clauses.push(
            "t.deleted_at IS NULL AND t.hidden = 0 AND t.archived = 0 AND t.status = 'open' AND t.priority <= 1".into(),
        ),
        "done" => clauses.push(
            "t.deleted_at IS NULL AND t.hidden = 0 AND t.archived = 0 AND t.status = 'done'".into(),
        ),
        "hidden" => clauses.push("t.deleted_at IS NULL AND t.hidden = 1 AND t.archived = 0".into()),
        "trash" => clauses.push("t.deleted_at IS NOT NULL".into()),
        "archived" => clauses.push("t.deleted_at IS NULL AND t.archived = 1".into()),
        // 全局搜索包含已完成与已隐藏事项；归档和回收站由各自视图访问。
        "search" => clauses.push("t.deleted_at IS NULL AND t.archived = 0".into()),
        other => {
            return Err(AppError::validation(format!("未知的视图：{other}")));
        }
    }

    if let Some(priority) = filter.priority {
        if !(PRIORITY_MIN..=PRIORITY_MAX).contains(&priority) {
            return Err(AppError::validation("优先级必须是 P0 ~ P3"));
        }
        clauses.push("t.priority = ?".into());
        values.push(Value::Integer(priority));
    }

    if let Some(category_id) = filter.category_id.as_deref().filter(|s| !s.is_empty()) {
        clauses.push("t.category_id = ?".into());
        values.push(Value::Text(category_id.to_string()));
    }

    if let Some(search) = filter
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let like = format!("%{}%", escape_like(search));
        // 标题、描述、负责部门，以及历史进展内容
        clauses.push(
            r#"(t.title LIKE ? ESCAPE '\'
                OR IFNULL(t.description, '') LIKE ? ESCAPE '\'
                OR IFNULL(t.department, '') LIKE ? ESCAPE '\'
                OR EXISTS (SELECT 1 FROM task_updates su
                            WHERE su.task_id = t.id AND su.content LIKE ? ESCAPE '\'))"#
                .into(),
        );
        for _ in 0..4 {
            values.push(Value::Text(like.clone()));
        }
    }

    // 日期范围按「记录时间」过滤，半开区间 [from, to)
    if view != "today" {
        if matches!((filter.date_from, filter.date_to), (Some(from), Some(to)) if from >= to) {
            return Err(AppError::validation("记录日期的起点必须早于终点"));
        }
        if let Some(from) = filter.date_from {
            clauses.push(format!("{TASK_DATE} >= ?"));
            values.push(Value::Integer(from));
        }
        if let Some(to) = filter.date_to {
            clauses.push(format!("{TASK_DATE} < ?"));
            values.push(Value::Integer(to));
        }
    }

    let order = match filter.sort.as_deref() {
        Some("created") => "t.created_at DESC".to_string(),
        Some("updated") => "t.updated_at DESC".to_string(),
        Some("recorded") => format!("{TASK_DATE} DESC, t.created_at DESC"),
        Some("manual") => "t.sort_order ASC, t.created_at DESC".to_string(),
        Some("priority") | None => {
            if view == "done" {
                "t.completed_at DESC, t.updated_at DESC".to_string()
            } else if view == "trash" {
                "t.deleted_at DESC".to_string()
            } else {
                "t.priority ASC, t.sort_order ASC, t.created_at DESC".to_string()
            }
        }
        Some(other) => {
            return Err(AppError::validation(format!("未知的排序方式：{other}")));
        }
    };

    Ok((clauses.join(" AND "), format!("{order}, t.id ASC"), values))
}

fn query_items(
    conn: &Connection,
    clauses: &str,
    order: &str,
    values: &[Value],
    limit: i64,
    offset: i64,
) -> AppResult<Vec<Task>> {
    let sql = format!("{SELECT_BASE} WHERE {clauses} ORDER BY {order} LIMIT ? OFFSET ?");
    let mut values = values.to_vec();
    values.push(Value::Integer(limit));
    values.push(Value::Integer(offset));

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(values.iter()), map_task)?;
    let mut tasks = Vec::new();
    for row in rows {
        tasks.push(row?);
    }
    Ok(tasks)
}

/// 兼容旧命令的数组响应与默认上限；新界面应使用 page 获取总数。
pub fn list(conn: &Connection, filter: &TaskFilter) -> AppResult<Vec<Task>> {
    let (clauses, order, values) = query_parts(filter)?;
    query_items(
        conn,
        &clauses,
        &order,
        &values,
        filter.limit.unwrap_or(500).clamp(1, 2000),
        filter.offset.unwrap_or(0).max(0),
    )
}

pub fn page(conn: &Connection, filter: &TaskFilter) -> AppResult<TaskPage> {
    let (clauses, order, values) = query_parts(filter)?;
    let total = conn.query_row(
        &format!("SELECT COUNT(*) FROM tasks t WHERE {clauses}"),
        params_from_iter(values.iter()),
        |row| row.get(0),
    )?;
    let limit = filter.limit.unwrap_or(50).clamp(1, 500);
    let offset = filter.offset.unwrap_or(0).max(0);
    let items = query_items(conn, &clauses, &order, &values, limit, offset)?;
    Ok(TaskPage {
        items,
        total,
        limit,
        offset,
    })
}

pub fn set_archived(conn: &Connection, id: &str, archived: bool) -> AppResult<Task> {
    crate::db::batch::set_one(
        conn,
        id,
        if archived {
            crate::db::batch::Action::Archive
        } else {
            crate::db::batch::Action::Unarchive
        },
    )?;
    get_required(conn, id)
}

/// before_id 为前一项，after_id 为后一项；锚点必须来自完整视图并且相邻。
pub fn move_task(conn: &Connection, input: &TaskMoveInput) -> AppResult<()> {
    if input.view == "search" {
        return Err(AppError::validation("搜索视图不能调整手动顺序"));
    }
    let (clauses, _, values) = query_parts(&TaskFilter {
        view: Some(input.view.clone()),
        ..Default::default()
    })?;
    let tx = conn.unchecked_transaction()?;
    let mut ids = {
        let mut stmt = tx.prepare(&format!("SELECT t.id FROM tasks t WHERE {clauses} ORDER BY t.sort_order ASC,t.created_at DESC,t.id ASC"))?;
        let rows = stmt.query_map(params_from_iter(values.iter()), |row| {
            row.get::<_, String>(0)
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    let index = ids
        .iter()
        .position(|id| id == &input.id)
        .ok_or_else(|| AppError::validation("移动事项不属于当前视图"))?;
    ids.remove(index);
    let position = |anchor: &String| {
        ids.iter()
            .position(|id| id == anchor)
            .ok_or_else(|| AppError::validation("排序锚点不属于当前视图，或与移动事项相同"))
    };
    let insertion = match (&input.before_id, &input.after_id) {
        (Some(before), Some(after)) => {
            let before = position(before)?;
            let after = position(after)?;
            if before + 1 != after {
                return Err(AppError::validation("排序锚点已发生变化，请刷新后重试"));
            }
            after
        }
        (Some(before), None) => position(before)? + 1,
        (None, Some(after)) => position(after)?,
        (None, None) if ids.is_empty() => 0,
        _ => return Err(AppError::validation("移动事项必须提供至少一个相邻锚点")),
    };
    ids.insert(insertion, input.id.clone());
    for (index, id) in ids.iter().enumerate() {
        tx.execute(
            "UPDATE tasks SET sort_order=? WHERE id=?",
            params![(index as i64 + 1) * 10, id],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn get(conn: &Connection, id: &str) -> AppResult<Option<Task>> {
    let sql = format!("{SELECT_BASE} WHERE t.id = ?");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query_map(params![id], map_task)?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

pub fn get_required(conn: &Connection, id: &str) -> AppResult<Task> {
    get(conn, id)?.ok_or_else(|| AppError::not_found("事项不存在或已被永久删除"))
}

pub fn create(conn: &Connection, input: &CreateTaskInput) -> AppResult<Task> {
    let title = validate_title(&input.title)?;
    let priority = input.priority.unwrap_or(2);
    if !(PRIORITY_MIN..=PRIORITY_MAX).contains(&priority) {
        return Err(AppError::validation("优先级必须是 P0 ~ P3"));
    }
    if let Some(desc) = input.description.as_deref() {
        if desc.chars().count() > DESC_MAX_CHARS {
            return Err(AppError::validation("事项描述过长"));
        }
    }

    let now = now_ms();
    let recorded_at = input.recorded_at.filter(|v| *v > 0).unwrap_or(now);
    let id = Uuid::new_v4().to_string();
    let sort_order: i64 = conn.query_row(
        "SELECT COALESCE(MAX(sort_order), 0) + 10 FROM tasks",
        [],
        |row| row.get(0),
    )?;

    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO tasks
            (id, title, description, priority, status, hidden, archived, category_id,
             department, recorded_at, sort_order, created_at, updated_at)
         VALUES (?, ?, ?, ?, 'open', 0, 0, ?, ?, ?, ?, ?, ?)",
        params![
            id,
            title,
            input
                .description
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty()),
            priority,
            input.category_id.as_deref().filter(|s| !s.is_empty()),
            input
                .department
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty()),
            recorded_at,
            sort_order,
            now,
            now,
        ],
    )?;
    tx.commit()?;

    get_required(conn, &id)
}

pub fn update(conn: &Connection, id: &str, patch: &TaskPatch) -> AppResult<Task> {
    let existing = get_required(conn, id)?;

    let mut sets: Vec<&str> = Vec::new();
    let mut values: Vec<Value> = Vec::new();

    if let Some(title) = patch.title.as_deref() {
        let clean = validate_title(title)?;
        sets.push("title = ?");
        values.push(Value::Text(clean));
    }
    if let Some(description) = patch.description.as_deref() {
        if description.chars().count() > DESC_MAX_CHARS {
            return Err(AppError::validation("事项描述过长"));
        }
        sets.push("description = ?");
        values.push(text_or_null(description));
    }
    if let Some(priority) = patch.priority {
        if !(PRIORITY_MIN..=PRIORITY_MAX).contains(&priority) {
            return Err(AppError::validation("优先级必须是 P0 ~ P3"));
        }
        sets.push("priority = ?");
        values.push(Value::Integer(priority));
    }
    if let Some(department) = patch.department.as_deref() {
        sets.push("department = ?");
        values.push(text_or_null(department));
    }
    if let Some(category_id) = patch.category_id.as_deref() {
        sets.push("category_id = ?");
        values.push(text_or_null(category_id));
    }
    if let Some(recorded_at) = patch.recorded_at {
        // <= 0 视为“不修改”，避免把记录时间误清空
        if recorded_at > 0 {
            sets.push("recorded_at = ?");
            values.push(Value::Integer(recorded_at));
        }
    }
    if let Some(sort_order) = patch.sort_order {
        sets.push("sort_order = ?");
        values.push(Value::Integer(sort_order));
    }

    if sets.is_empty() {
        return Ok(existing);
    }

    sets.push("updated_at = ?");
    values.push(Value::Integer(now_ms()));
    values.push(Value::Text(id.to_string()));

    let sql = format!("UPDATE tasks SET {} WHERE id = ?", sets.join(", "));
    conn.execute(&sql, params_from_iter(values.iter()))?;

    get_required(conn, id)
}

pub fn set_completed(conn: &Connection, id: &str, completed: bool) -> AppResult<Task> {
    crate::db::batch::set_one(
        conn,
        id,
        if completed {
            crate::db::batch::Action::Complete
        } else {
            crate::db::batch::Action::Reopen
        },
    )?;
    get_required(conn, id)
}

pub fn set_hidden(conn: &Connection, id: &str, hidden: bool) -> AppResult<Task> {
    crate::db::batch::set_one(
        conn,
        id,
        if hidden {
            crate::db::batch::Action::Hide
        } else {
            crate::db::batch::Action::Unhide
        },
    )?;
    get_required(conn, id)
}

pub fn trash(conn: &Connection, id: &str) -> AppResult<()> {
    crate::db::batch::set_one(conn, id, crate::db::batch::Action::Trash)
}

pub fn restore(conn: &Connection, id: &str) -> AppResult<Task> {
    crate::db::batch::set_one(conn, id, crate::db::batch::Action::Restore)?;
    get_required(conn, id)
}

pub fn delete_permanent(conn: &Connection, id: &str) -> AppResult<()> {
    // v3 交接条目通过 SET NULL 保留独立快照。
    crate::db::batch::set_one(conn, id, crate::db::batch::Action::DeletePermanent)
}

pub fn duplicate(conn: &Connection, id: &str) -> AppResult<Task> {
    let source = get_required(conn, id)?;
    let input = CreateTaskInput {
        title: format!("{}（副本）", source.title),
        description: source.description.clone(),
        priority: Some(source.priority),
        department: source.department.clone(),
        category_id: source.category_id.clone(),
        // 副本是一条新记录，记录时间取当前时间
        recorded_at: None,
    };
    create(conn, &input)
}

pub fn reorder(conn: &Connection, ordered_ids: &[String]) -> AppResult<()> {
    let tx = conn.unchecked_transaction()?;
    let (clauses, _, values) = query_parts(&TaskFilter::default())?;
    let scope: std::collections::HashSet<String> = {
        let mut stmt = tx.prepare(&format!("SELECT t.id FROM tasks t WHERE {clauses}"))?;
        let rows = stmt.query_map(params_from_iter(values.iter()), |row| {
            row.get::<_, String>(0)
        })?;
        rows.collect::<Result<_, _>>()?
    };
    let supplied: std::collections::HashSet<_> = ordered_ids.iter().cloned().collect();
    if supplied != scope || supplied.len() != ordered_ids.len() {
        return Err(AppError::validation(
            "旧排序接口必须提交完整事项范围；分页拖动请使用移动接口",
        ));
    }
    for (index, id) in ordered_ids.iter().enumerate() {
        tx.execute(
            "UPDATE tasks SET sort_order = ? WHERE id = ?",
            params![((index as i64) + 1) * 10, id],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn counts(conn: &Connection) -> AppResult<Counts> {
    let (today_start, today_end) = local_day_bounds(0);
    let row = conn.query_row(
        "SELECT
            (SELECT COUNT(*) FROM tasks WHERE deleted_at IS NULL AND hidden = 0 AND archived = 0),
            (SELECT COUNT(*) FROM tasks WHERE deleted_at IS NULL AND hidden = 0 AND archived = 0
                AND COALESCE(recorded_at, created_at) >= ?1
                AND COALESCE(recorded_at, created_at) < ?2),
            (SELECT COUNT(*) FROM tasks WHERE deleted_at IS NULL AND hidden = 0 AND archived = 0 AND status = 'open'),
            (SELECT COUNT(*) FROM tasks WHERE deleted_at IS NULL AND hidden = 0 AND archived = 0 AND status = 'open' AND priority <= 1),
            (SELECT COUNT(*) FROM tasks WHERE deleted_at IS NULL AND hidden = 0 AND archived = 0 AND status = 'done'),
            (SELECT COUNT(*) FROM tasks WHERE deleted_at IS NULL AND hidden = 1 AND archived = 0),
            (SELECT COUNT(*) FROM tasks WHERE deleted_at IS NOT NULL),
            (SELECT COUNT(*) FROM tasks WHERE deleted_at IS NULL AND archived = 1)",
        params![today_start, today_end],
        |r| {
            Ok(Counts {
                all: r.get(0)?,
                today: r.get(1)?,
                open: r.get(2)?,
                focus: r.get(3)?,
                done: r.get(4)?,
                hidden: r.get(5)?,
                trash: r.get(6)?,
                archived: r.get(7)?,
            })
        },
    )?;
    Ok(row)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::updates;
    use crate::db::Db;

    fn db() -> Db {
        Db::open_in_memory().expect("内存数据库")
    }

    fn new_task(db: &Db, title: &str, priority: i64) -> Task {
        db.with_conn(|conn| {
            create(
                conn,
                &CreateTaskInput {
                    title: title.to_string(),
                    priority: Some(priority),
                    ..Default::default()
                },
            )
        })
        .unwrap()
    }

    fn list_view(db: &Db, view: &str) -> Vec<Task> {
        db.with_conn(|conn| {
            list(
                conn,
                &TaskFilter {
                    view: Some(view.into()),
                    ..Default::default()
                },
            )
        })
        .unwrap()
    }

    #[test]
    fn create_and_read_task() {
        let db = db();
        let task = new_task(&db, "  生产服务器异常排查  ", 0);
        assert_eq!(task.title, "生产服务器异常排查");
        assert_eq!(task.priority, 0);
        assert_eq!(task.status, "open");
        assert!(!task.hidden);
        assert!(task.deleted_at.is_none());
        assert!(task.created_at > 0);
        // 未指定记录时间时默认取当前时间
        assert!(task.recorded_at >= task.created_at - 5_000);
    }

    #[test]
    fn rejects_empty_title() {
        let db = db();
        let err = db
            .with_conn(|conn| {
                create(
                    conn,
                    &CreateTaskInput {
                        title: "   ".into(),
                        ..Default::default()
                    },
                )
            })
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn stores_department_and_custom_recorded_at() {
        let db = db();
        let recorded = 1_700_000_000_000_i64;
        let task = db
            .with_conn(|conn| {
                create(
                    conn,
                    &CreateTaskInput {
                        title: "夜间巡检".into(),
                        department: Some("  运维二部 ".into()),
                        recorded_at: Some(recorded),
                        ..Default::default()
                    },
                )
            })
            .unwrap();

        assert_eq!(task.department.as_deref(), Some("运维二部"));
        assert_eq!(task.recorded_at, recorded);

        let updated = db
            .with_conn(|conn| {
                update(
                    conn,
                    &task.id,
                    &TaskPatch {
                        recorded_at: Some(recorded + 3_600_000),
                        department: Some("".into()),
                        ..Default::default()
                    },
                )
            })
            .unwrap();
        assert_eq!(updated.recorded_at, recorded + 3_600_000);
        assert!(updated.department.is_none(), "空字符串应清空负责部门");
    }

    #[test]
    fn filters_by_recorded_date_not_created_date() {
        let db = db();
        let today = local_day_bounds(0).0;
        let yesterday = local_day_bounds(-1).0;

        let old = db
            .with_conn(|conn| {
                create(
                    conn,
                    &CreateTaskInput {
                        title: "昨天的记录".into(),
                        recorded_at: Some(yesterday + 3_600_000),
                        ..Default::default()
                    },
                )
            })
            .unwrap();
        let fresh = db
            .with_conn(|conn| {
                create(
                    conn,
                    &CreateTaskInput {
                        title: "今天的记录".into(),
                        recorded_at: Some(today + 3_600_000),
                        ..Default::default()
                    },
                )
            })
            .unwrap();

        let todays = list_view(&db, "today");
        assert_eq!(todays.len(), 1);
        assert_eq!(todays[0].id, fresh.id);

        // 自定义日期范围：只取昨天
        let only_yesterday = db
            .with_conn(|conn| {
                let (start, end) = local_day_bounds(-1);
                list(
                    conn,
                    &TaskFilter {
                        date_from: Some(start),
                        date_to: Some(end),
                        ..Default::default()
                    },
                )
            })
            .unwrap();
        assert_eq!(only_yesterday.len(), 1);
        assert_eq!(only_yesterday[0].id, old.id);
    }

    #[test]
    fn list_filters_by_priority_and_view() {
        let db = db();
        new_task(&db, "P0 故障", 0);
        new_task(&db, "P2 常规", 2);
        let done = new_task(&db, "已完成", 1);
        db.with_conn(|conn| set_completed(conn, &done.id, true))
            .unwrap();

        let open = list_view(&db, "open");
        assert_eq!(open.len(), 2);
        // 默认按优先级排序，P0 在最前
        assert_eq!(open[0].priority, 0);

        let focus = list_view(&db, "focus");
        assert_eq!(focus.len(), 1);
        assert_eq!(focus[0].title, "P0 故障");
    }

    #[test]
    fn completing_task_disables_reminder() {
        let db = db();
        let task = new_task(&db, "带提醒的事项", 1);
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO reminders (task_id, enabled, interval_minutes, next_remind_at, created_at, updated_at)
                 VALUES (?, 1, 30, ?, ?, ?)",
                params![task.id, now_ms() + 60_000, now_ms(), now_ms()],
            )?;
            Ok(())
        })
        .unwrap();

        let completed = db
            .with_conn(|conn| set_completed(conn, &task.id, true))
            .unwrap();
        assert_eq!(completed.status, "done");
        assert!(!completed.reminder_enabled);
        assert!(completed.next_remind_at.is_none());
    }

    #[test]
    fn hiding_removes_from_default_list_but_keeps_progress() {
        let db = db();
        let task = new_task(&db, "隐藏事项", 2);
        db.with_conn(|conn| updates::add(conn, &task.id, "第一条进展", Some("张三")))
            .unwrap();
        db.with_conn(|conn| set_hidden(conn, &task.id, true))
            .unwrap();

        assert!(list_view(&db, "all").is_empty());

        let hidden = list_view(&db, "hidden");
        assert_eq!(hidden.len(), 1);
        assert_eq!(hidden[0].update_count, 1);
        assert_eq!(
            hidden[0].latest_update_content.as_deref(),
            Some("第一条进展")
        );
    }

    #[test]
    fn trash_restore_and_permanent_delete() {
        let db = db();
        let task = new_task(&db, "待删除", 3);
        db.with_conn(|conn| updates::add(conn, &task.id, "进展", None))
            .unwrap();

        db.with_conn(|conn| trash(conn, &task.id)).unwrap();
        assert_eq!(list_view(&db, "trash").len(), 1);

        let restored = db.with_conn(|conn| restore(conn, &task.id)).unwrap();
        assert!(restored.deleted_at.is_none());

        db.with_conn(|conn| trash(conn, &task.id)).unwrap();
        db.with_conn(|conn| delete_permanent(conn, &task.id))
            .unwrap();

        let orphan: i64 = db
            .with_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM task_updates WHERE task_id = ?",
                    params![task.id],
                    |row| row.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(orphan, 0, "永久删除事项时必须级联清理进展记录");
    }

    #[test]
    fn search_matches_title_description_department_and_progress() {
        let db = db();
        let task = db
            .with_conn(|conn| {
                create(
                    conn,
                    &CreateTaskInput {
                        title: "设备巡检".into(),
                        description: Some("检查机房空调".into()),
                        department: Some("运维二部".into()),
                        ..Default::default()
                    },
                )
            })
            .unwrap();
        db.with_conn(|conn| updates::add(conn, &task.id, "已联系数据库维护人员", None))
            .unwrap();

        let count = |needle: &str| {
            db.with_conn(|conn| {
                list(
                    conn,
                    &TaskFilter {
                        search: Some(needle.into()),
                        ..Default::default()
                    },
                )
            })
            .unwrap()
            .len()
        };

        assert_eq!(count("巡检"), 1, "标题");
        assert_eq!(count("空调"), 1, "描述");
        assert_eq!(count("运维二部"), 1, "负责部门");
        assert_eq!(count("数据库维护"), 1, "历史进展");
    }

    #[test]
    fn search_wildcards_are_escaped() {
        let db = db();
        new_task(&db, "百分之百", 2);
        let found = db
            .with_conn(|conn| {
                list(
                    conn,
                    &TaskFilter {
                        search: Some("%".into()),
                        ..Default::default()
                    },
                )
            })
            .unwrap();
        assert!(found.is_empty(), "% 应被转义，不能匹配任意内容");
    }

    #[test]
    fn completed_task_can_be_restored() {
        let db = db();
        let task = new_task(&db, "会完成的事项", 2);
        db.with_conn(|conn| set_completed(conn, &task.id, true))
            .unwrap();
        let reopened = db
            .with_conn(|conn| set_completed(conn, &task.id, false))
            .unwrap();
        assert_eq!(reopened.status, "open");
        assert!(reopened.completed_at.is_none());
    }

    #[test]
    fn duplicate_copies_fields_without_progress() {
        let db = db();
        let task = db
            .with_conn(|conn| {
                create(
                    conn,
                    &CreateTaskInput {
                        title: "原始事项".into(),
                        priority: Some(1),
                        department: Some("调度中心".into()),
                        ..Default::default()
                    },
                )
            })
            .unwrap();
        db.with_conn(|conn| updates::add(conn, &task.id, "进展 A", None))
            .unwrap();

        let copy = db.with_conn(|conn| duplicate(conn, &task.id)).unwrap();
        assert_eq!(copy.title, "原始事项（副本）");
        assert_eq!(copy.update_count, 0);
        assert_eq!(copy.priority, 1);
        assert_eq!(copy.department.as_deref(), Some("调度中心"));
        assert!(
            copy.recorded_at >= task.recorded_at,
            "副本应使用新的记录时间"
        );
    }

    #[test]
    fn sorts_by_recorded_time_desc() {
        let db = db();
        let base = local_day_bounds(0).0;
        for (index, title) in ["最早", "中间", "最新"].iter().enumerate() {
            db.with_conn(|conn| {
                create(
                    conn,
                    &CreateTaskInput {
                        title: (*title).into(),
                        priority: Some(2),
                        recorded_at: Some(base + (index as i64) * 60_000),
                        ..Default::default()
                    },
                )
            })
            .unwrap();
        }

        let sorted = db
            .with_conn(|conn| {
                list(
                    conn,
                    &TaskFilter {
                        sort: Some("recorded".into()),
                        ..Default::default()
                    },
                )
            })
            .unwrap();
        assert_eq!(sorted[0].title, "最新");
        assert_eq!(sorted[2].title, "最早");
    }
}
