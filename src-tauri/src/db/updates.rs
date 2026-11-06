//! 事项进展记录数据访问层。
//!
//! 设计要点：进展只能追加，编辑保留 `created_at` 并写入 `edited_at`，
//! 绝不覆盖历史记录。

use rusqlite::{params, Connection, Row};
use uuid::Uuid;

use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::models::TaskUpdate;

const CONTENT_MAX_CHARS: usize = 4000;
const AUTHOR_MAX_CHARS: usize = 50;

fn map_update(row: &Row<'_>) -> rusqlite::Result<TaskUpdate> {
    Ok(TaskUpdate {
        id: row.get(0)?,
        task_id: row.get(1)?,
        content: row.get(2)?,
        author: row.get(3)?,
        kind: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
        edited_at: row.get(7)?,
    })
}

/// 按时间倒序返回某事项的全部进展（最新在前）。
pub fn list(conn: &Connection, task_id: &str) -> AppResult<Vec<TaskUpdate>> {
    let mut stmt = conn.prepare(
        "SELECT id, task_id, content, author, kind, created_at, updated_at, edited_at
           FROM task_updates
          WHERE task_id = ?
          ORDER BY created_at DESC, rowid DESC",
    )?;
    let rows = stmt.query_map(params![task_id], map_update)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn get(conn: &Connection, id: &str) -> AppResult<Option<TaskUpdate>> {
    let mut stmt = conn.prepare(
        "SELECT id, task_id, content, author, kind, created_at, updated_at, edited_at
           FROM task_updates WHERE id = ?",
    )?;
    let mut rows = stmt.query_map(params![id], map_update)?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

fn validate_content(content: &str) -> AppResult<String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err(AppError::validation("进展内容不能为空"));
    }
    if trimmed.chars().count() > CONTENT_MAX_CHARS {
        return Err(AppError::validation(format!(
            "单条进展不能超过 {CONTENT_MAX_CHARS} 个字符"
        )));
    }
    Ok(trimmed.to_string())
}

fn normalize_author(author: Option<&str>) -> Option<String> {
    author
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.chars().take(AUTHOR_MAX_CHARS).collect())
}

pub fn add(
    conn: &Connection,
    task_id: &str,
    content: &str,
    author: Option<&str>,
) -> AppResult<TaskUpdate> {
    let content = validate_content(content)?;

    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE id = ? AND deleted_at IS NULL",
        params![task_id],
        |row| row.get(0),
    )?;
    if exists == 0 {
        return Err(AppError::not_found("事项不存在或已在回收站中"));
    }

    let now = now_ms();
    let id = Uuid::new_v4().to_string();
    let author = normalize_author(author);

    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO task_updates (id, task_id, content, author, kind, created_at, updated_at)
         VALUES (?, ?, ?, ?, 'progress', ?, ?)",
        params![id, task_id, content, author, now, now],
    )?;
    // 让列表的“最近更新”反映最新进展时间
    tx.execute(
        "UPDATE tasks SET updated_at = ? WHERE id = ?",
        params![now, task_id],
    )?;
    tx.commit()?;

    get(conn, &id)?.ok_or_else(|| AppError::Other("进展保存失败".into()))
}

pub fn edit(conn: &Connection, id: &str, content: &str) -> AppResult<TaskUpdate> {
    let content = validate_content(content)?;
    let existing = get(conn, id)?.ok_or_else(|| AppError::not_found("进展记录不存在"))?;
    if existing.kind != "progress" {
        return Err(AppError::validation("系统记录不可编辑"));
    }

    let now = now_ms();
    conn.execute(
        "UPDATE task_updates SET content = ?, updated_at = ?, edited_at = ? WHERE id = ?",
        params![content, now, now, id],
    )?;
    // 保留创建时间，同时刷新所属事项的更新时间
    conn.execute(
        "UPDATE tasks SET updated_at = ? WHERE id = ?",
        params![now, existing.task_id],
    )?;

    get(conn, id)?.ok_or_else(|| AppError::not_found("进展记录不存在"))
}

pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    let existing = get(conn, id)?.ok_or_else(|| AppError::not_found("进展记录不存在"))?;
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM task_updates WHERE id = ?", params![id])?;
    tx.execute(
        "UPDATE tasks SET updated_at = ? WHERE id = ?",
        params![now_ms(), existing.task_id],
    )?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tasks::create;
    use crate::db::Db;
    use crate::models::CreateTaskInput;

    fn setup() -> (Db, String) {
        let db = Db::open_in_memory().unwrap();
        let id = db
            .with_conn(|conn| {
                let t = create(
                    conn,
                    &CreateTaskInput {
                        title: "进展测试".into(),
                        priority: Some(2),
                        ..Default::default()
                    },
                )?;
                Ok(t.id)
            })
            .unwrap();
        (db, id)
    }

    #[test]
    fn appends_never_overwrite_history() {
        let (db, task_id) = setup();
        db.with_conn(|conn| add(conn, &task_id, "第一条", Some("张三")))
            .unwrap();
        db.with_conn(|conn| add(conn, &task_id, "第二条", None))
            .unwrap();

        let items = db.with_conn(|conn| list(conn, &task_id)).unwrap();
        assert_eq!(items.len(), 2);
        // 最新的在最前
        assert_eq!(items[0].content, "第二条");
        assert_eq!(items[1].content, "第一条");
        assert_eq!(items[1].author.as_deref(), Some("张三"));
    }

    #[test]
    fn edit_keeps_created_at_and_sets_edited_at() {
        let (db, task_id) = setup();
        let created = db
            .with_conn(|conn| add(conn, &task_id, "原始内容", None))
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let edited = db
            .with_conn(|conn| edit(conn, &created.id, "修改后的内容"))
            .unwrap();

        assert_eq!(edited.content, "修改后的内容");
        assert_eq!(edited.created_at, created.created_at, "创建时间必须保留");
        assert!(edited.edited_at.is_some(), "必须记录修改时间");
    }

    #[test]
    fn rejects_blank_content() {
        let (db, task_id) = setup();
        let err = db
            .with_conn(|conn| add(conn, &task_id, "   ", None))
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn deleting_update_keeps_task() {
        let (db, task_id) = setup();
        let item = db
            .with_conn(|conn| add(conn, &task_id, "将被删除", None))
            .unwrap();
        db.with_conn(|conn| delete(conn, &item.id)).unwrap();
        assert!(db
            .with_conn(|conn| list(conn, &task_id))
            .unwrap()
            .is_empty());
        assert!(db
            .with_conn(|conn| crate::db::tasks::get(conn, &task_id))
            .unwrap()
            .is_some());
    }
}
