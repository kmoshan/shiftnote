//! 批量事项操作：明确目标状态，在同一事务中提交。

use std::collections::HashSet;

use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::models::{TaskBatchInput, TaskBatchResult};
use rusqlite::{params, Connection, OptionalExtension};

#[derive(Clone, Copy)]
pub(crate) enum Action {
    Complete,
    Reopen,
    Hide,
    Unhide,
    Priority(i64),
    Trash,
    Restore,
    Archive,
    Unarchive,
    DeletePermanent,
}

struct TaskState {
    id: String,
    status: String,
    priority: i64,
    hidden: bool,
    archived: bool,
    deleted_at: Option<i64>,
    completed_at: Option<i64>,
}

fn load(conn: &Connection, id: &str) -> AppResult<Option<TaskState>> {
    Ok(conn.query_row("SELECT id,status,priority,hidden,archived,deleted_at,completed_at FROM tasks WHERE id=?", [id], |row| Ok(TaskState {
        id: row.get(0)?, status: row.get(1)?, priority: row.get(2)?, hidden: row.get::<_, i64>(3)? != 0,
        archived: row.get::<_, i64>(4)? != 0, deleted_at: row.get(5)?, completed_at: row.get(6)?,
    })).optional()?)
}

impl Action {
    fn requirement(self) -> &'static str {
        match self {
            Self::Restore => "该事项不在回收站中",
            Self::DeletePermanent => "只能永久删除回收站中的事项",
            Self::Archive => "仅已完成且不在回收站的事项可以归档",
            _ => "事项已移入回收站，请先恢复",
        }
    }

    fn valid(self, task: &TaskState) -> bool {
        match self {
            Self::Restore | Self::DeletePermanent => task.deleted_at.is_some(),
            Self::Archive => task.deleted_at.is_none() && task.status == "done",
            _ => task.deleted_at.is_none(),
        }
    }

    fn changes(self, task: &TaskState) -> bool {
        match self {
            Self::Complete => task.status != "done",
            Self::Reopen => task.status != "open" || task.archived || task.completed_at.is_some(),
            Self::Hide => !task.hidden,
            Self::Unhide => task.hidden,
            Self::Priority(priority) => task.priority != priority,
            Self::Archive => !task.archived,
            Self::Unarchive => task.archived,
            Self::Trash | Self::Restore | Self::DeletePermanent => true,
        }
    }
}

fn apply(conn: &Connection, task: &TaskState, action: Action, now: i64) -> AppResult<bool> {
    let changed = action.changes(task);
    if changed {
        let id = &task.id;
        match action {
            Action::Complete => {
                conn.execute(
                    "UPDATE tasks SET status='done',completed_at=?,updated_at=? WHERE id=?",
                    params![now, now, id],
                )?;
            }
            Action::Reopen => {
                conn.execute("UPDATE tasks SET status='open',completed_at=NULL,archived=0,updated_at=? WHERE id=?", params![now,id])?;
            }
            Action::Hide | Action::Unhide => {
                conn.execute(
                    "UPDATE tasks SET hidden=?,updated_at=? WHERE id=?",
                    params![matches!(action, Action::Hide) as i64, now, id],
                )?;
            }
            Action::Priority(priority) => {
                conn.execute(
                    "UPDATE tasks SET priority=?,updated_at=? WHERE id=?",
                    params![priority, now, id],
                )?;
            }
            Action::Trash => {
                conn.execute(
                    "UPDATE tasks SET deleted_at=?,updated_at=? WHERE id=?",
                    params![now, now, id],
                )?;
            }
            Action::Restore => {
                conn.execute(
                    "UPDATE tasks SET deleted_at=NULL,hidden=0,updated_at=? WHERE id=?",
                    params![now, id],
                )?;
            }
            Action::Archive | Action::Unarchive => {
                conn.execute(
                    "UPDATE tasks SET archived=?,updated_at=? WHERE id=?",
                    params![matches!(action, Action::Archive) as i64, now, id],
                )?;
            }
            Action::DeletePermanent => {
                conn.execute("DELETE FROM tasks WHERE id=?", [id])?;
            }
        }
    }
    if matches!(
        action,
        Action::Complete | Action::Hide | Action::Trash | Action::Archive
    ) {
        conn.execute("UPDATE reminders SET enabled=0,next_remind_at=NULL,updated_at=? WHERE task_id=? AND (enabled<>0 OR next_remind_at IS NOT NULL)", params![now,task.id])?;
    }
    Ok(changed)
}

pub(crate) fn set_one(conn: &Connection, id: &str, action: Action) -> AppResult<()> {
    let tx = conn.unchecked_transaction()?;
    let task = load(&tx, id)?.ok_or_else(|| AppError::not_found("事项不存在或已被永久删除"))?;
    if !action.valid(&task) {
        return Err(AppError::validation(action.requirement()));
    }
    apply(&tx, &task, action, now_ms())?;
    tx.commit()?;
    Ok(())
}

fn invalid(message: &str, invalid_ids: Vec<String>) -> AppError {
    AppError::Batch {
        message: message.into(),
        invalid_ids,
    }
}

pub fn execute(conn: &Connection, input: &TaskBatchInput) -> AppResult<TaskBatchResult> {
    let mut seen = HashSet::new();
    let ids: Vec<_> = input
        .ids
        .iter()
        .filter(|id| seen.insert((*id).clone()))
        .collect();
    if ids.is_empty() || ids.len() > 1000 {
        return Err(invalid("批量操作需选择 1 至 1000 个不同事项", Vec::new()));
    }
    let action = match input.action.as_str() {
        "complete" => Action::Complete,
        "reopen" => Action::Reopen,
        "hide" => Action::Hide,
        "unhide" => Action::Unhide,
        "priority" => match input.priority {
            Some(p @ 0..=3) => Action::Priority(p),
            _ => return Err(invalid("优先级必须是 P0 ~ P3", Vec::new())),
        },
        "trash" => Action::Trash,
        "restore" => Action::Restore,
        "archive" => Action::Archive,
        "unarchive" => Action::Unarchive,
        "deletePermanent" => Action::DeletePermanent,
        _ => return Err(invalid("未知的批量操作", Vec::new())),
    };
    let tx = conn.unchecked_transaction()?;
    let mut states = Vec::new();
    let mut invalid_ids = Vec::new();
    for id in &ids {
        match load(&tx, id)? {
            Some(task) if action.valid(&task) => states.push(task),
            _ => invalid_ids.push((*id).clone()),
        }
    }
    if !invalid_ids.is_empty() {
        return Err(invalid(
            &format!(
                "部分事项不存在或状态不允许此操作（{}），整批操作未执行",
                action.requirement()
            ),
            invalid_ids,
        ));
    }
    let now = now_ms();
    let mut changed = 0;
    for state in states {
        if apply(&tx, &state, action, now)? {
            changed += 1;
        }
    }
    tx.commit()?;
    let requested = ids.len() as i64;
    Ok(TaskBatchResult {
        requested,
        changed,
        unchanged: requested - changed,
    })
}

#[cfg(test)]
#[path = "batch_tests.rs"]
mod tests;
