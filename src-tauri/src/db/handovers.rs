//! 交接单：草稿读取实时事项，定稿在同一事务中冻结独立快照。

use std::collections::HashSet;

use chrono::{Local, TimeZone};
use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension, Row};
use uuid::Uuid;

use crate::db::{now_ms, tasks, updates};
use crate::error::{AppError, AppResult};
use crate::models::*;

#[cfg(test)]
#[path = "handover_tests.rs"]
mod tests;

pub const SECTIONS: &[(&str, &str)] = &[
    ("new", "本班新增"),
    ("handled", "本班已处理"),
    ("pending", "当前待处理"),
    ("follow_up", "后续跟进"),
    ("focus", "重点关注"),
];

const HEADER_COLUMNS: &str = "id, title, handover_date, shift_from, shift_to, note, period_start, period_end, status, finalized_at, snapshot_version, created_at, updated_at";

fn map_header(r: &Row<'_>) -> rusqlite::Result<Handover> {
    Ok(Handover {
        id: r.get(0)?,
        title: r.get(1)?,
        handover_date: r.get(2)?,
        shift_from: r.get(3)?,
        shift_to: r.get(4)?,
        note: r.get(5)?,
        period_start: r.get(6)?,
        period_end: r.get(7)?,
        status: r.get(8)?,
        finalized_at: r.get(9)?,
        snapshot_version: r.get(10)?,
        created_at: r.get(11)?,
        updated_at: r.get(12)?,
    })
}

fn header(conn: &Connection, id: &str) -> AppResult<Handover> {
    conn.query_row(
        &format!("SELECT {HEADER_COLUMNS} FROM handovers WHERE id=?"),
        [id],
        map_header,
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("交接记录不存在"))
}

fn json<T: serde::Serialize>(value: &T) -> AppResult<String> {
    serde_json::to_string(value).map_err(|e| AppError::Other(format!("交接快照序列化失败：{e}")))
}

fn parse_json<T: serde::de::DeserializeOwned>(value: &str) -> AppResult<T> {
    serde_json::from_str(value).map_err(|e| AppError::validation(format!("交接快照格式无效：{e}")))
}

fn period(start: i64, end: i64) -> AppResult<()> {
    if start >= end {
        return Err(AppError::validation("交接结束时间必须晚于开始时间"));
    }
    if Local.timestamp_millis_opt(start).single().is_none()
        || Local.timestamp_millis_opt(end).single().is_none()
    {
        return Err(AppError::validation("交接时间超出有效范围"));
    }
    Ok(())
}

fn optional_text(value: &Option<String>, max: usize, name: &str) -> AppResult<Option<String>> {
    let text = value.as_deref().map(str::trim).filter(|s| !s.is_empty());
    if text.is_some_and(|s| s.chars().count() > max) {
        return Err(AppError::validation(format!("{name}过长")));
    }
    Ok(text.map(str::to_string))
}

fn draft_snapshot(
    conn: &Connection,
    task_id: &str,
    selected_ids: &[String],
    start: i64,
    end: i64,
    strict: bool,
) -> AppResult<TaskSnapshot> {
    let task = tasks::get_required(conn, task_id)?;
    if strict && task.deleted_at.is_some() {
        return Err(AppError::validation(format!(
            "事项「{}」已在回收站，请恢复后交接",
            task.title
        )));
    }
    let wanted: HashSet<&str> = selected_ids.iter().map(String::as_str).collect();
    let mut selected = updates::list(conn, task_id)?
        .into_iter()
        .filter(|u| wanted.contains(u.id.as_str()))
        .collect::<Vec<_>>();
    if strict
        && (selected.len() != wanted.len()
            || selected
                .iter()
                .any(|u| u.kind != "progress" || u.created_at < start || u.created_at >= end))
    {
        return Err(AppError::validation(format!(
            "事项「{}」的所选进展不存在、不属于该事项或超出交接区间，请重新选择",
            task.title
        )));
    }
    selected.retain(|u| u.kind == "progress" && u.created_at >= start && u.created_at < end);
    selected.sort_by(|a, b| a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id)));
    Ok(TaskSnapshot {
        task,
        updates: selected,
    })
}

fn missing_legacy_snapshot(item_id: &str, h: &Handover) -> TaskSnapshot {
    TaskSnapshot {
        task: Task {
            id: format!("missing:{item_id}"),
            title: "原事项已永久删除（历史草稿）".into(),
            description: Some("旧版关联未保存事项内容，请明确移除此条后再保存或定稿。".into()),
            priority: 2,
            status: "open".into(),
            completed_at: None,
            hidden: false,
            archived: false,
            deleted_at: Some(h.updated_at),
            category_id: None,
            department: None,
            recorded_at: h.period_start,
            sort_order: 0,
            created_at: h.created_at,
            updated_at: h.updated_at,
            latest_update_content: None,
            latest_update_at: None,
            update_count: 0,
            reminder_enabled: false,
            next_remind_at: None,
        },
        updates: vec![],
    }
}

pub fn get(conn: &Connection, id: &str) -> AppResult<HandoverDetail> {
    let handover = header(conn, id)?;
    if handover.snapshot_version != 1 {
        return Err(AppError::validation(
            "此交接快照来自不支持的格式版本，请升级应用",
        ));
    }
    let mut stmt = conn.prepare("SELECT id, task_id, section, note, sort_order, selected_update_ids_json, snapshot_json FROM handover_tasks WHERE handover_id=? ORDER BY CASE section WHEN 'new' THEN 0 WHEN 'handled' THEN 1 WHEN 'pending' THEN 2 WHEN 'follow_up' THEN 3 WHEN 'focus' THEN 4 END, sort_order, id")?;
    let rows = stmt.query_map([id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, String>(5)?,
            r.get::<_, Option<String>>(6)?,
        ))
    })?;
    let mut items = Vec::new();
    for row in rows {
        let (id, task_id, section, note, sort_order, selection, frozen) = row?;
        let selected_update_ids: Vec<String> = parse_json(&selection)?;
        let snapshot = if handover.status == "finalized" {
            parse_json(
                frozen
                    .as_deref()
                    .ok_or_else(|| AppError::validation("已定稿交接缺少快照，无法安全读取"))?,
            )?
        } else if let Some(task_id) = task_id.as_deref() {
            draft_snapshot(
                conn,
                task_id,
                &selected_update_ids,
                handover.period_start,
                handover.period_end,
                false,
            )?
        } else if let Some(saved) = frozen.as_deref() {
            parse_json(saved)?
        } else {
            missing_legacy_snapshot(&id, &handover)
        };
        items.push(HandoverItem {
            id,
            task_id,
            section,
            note,
            sort_order,
            selected_update_ids,
            snapshot,
        });
    }
    Ok(HandoverDetail { handover, items })
}

pub fn list(conn: &Connection, filter: &HandoverFilter) -> AppResult<HandoverPage> {
    let mut clauses = vec!["1=1".to_string()];
    let mut values = Vec::<Value>::new();
    if let Some(status) = filter.status.as_deref().filter(|s| !s.is_empty()) {
        if !matches!(status, "draft" | "finalized") {
            return Err(AppError::validation("未知的交接状态"));
        }
        clauses.push("status=?".into());
        values.push(status.to_string().into());
    }
    if let Some(search) = filter
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        clauses.push("(title LIKE ? ESCAPE '\\' OR IFNULL(note,'') LIKE ? ESCAPE '\\' OR IFNULL(shift_from,'') LIKE ? ESCAPE '\\' OR IFNULL(shift_to,'') LIKE ? ESCAPE '\\')".into());
        let like = format!("%{}%", escape_like(search));
        for _ in 0..4 {
            values.push(like.clone().into());
        }
    }
    let where_sql = clauses.join(" AND ");
    let total = conn.query_row(
        &format!("SELECT COUNT(*) FROM handovers WHERE {where_sql}"),
        params_from_iter(values.iter()),
        |r| r.get(0),
    )?;
    let limit = filter.limit.unwrap_or(50).clamp(1, 2000);
    let offset = filter.offset.unwrap_or(0).max(0);
    values.extend([Value::Integer(limit), Value::Integer(offset)]);
    let mut stmt = conn.prepare(&format!("SELECT {HEADER_COLUMNS} FROM handovers WHERE {where_sql} ORDER BY period_start DESC, created_at DESC, id ASC LIMIT ? OFFSET ?"))?;
    let items = stmt
        .query_map(params_from_iter(values.iter()), map_header)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(HandoverPage {
        items,
        total,
        limit,
        offset,
    })
}

fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

pub fn candidates(conn: &Connection, filter: &HandoverCandidateFilter) -> AppResult<TaskPage> {
    period(filter.period_start, filter.period_end)?;
    let mut clauses = vec!["t.hidden=0 AND t.archived=0 AND t.deleted_at IS NULL".to_string()];
    let mut values = Vec::<Value>::new();
    match filter.kind.as_str() {
        "all" => {}
        "pending" => clauses.push("t.status='open'".into()),
        "new" => {
            clauses.push("COALESCE(t.recorded_at,t.created_at)>=? AND COALESCE(t.recorded_at,t.created_at)<?".into());
            values.extend([filter.period_start.into(), filter.period_end.into()]);
        }
        "handled" => {
            clauses.push("t.status='done' AND t.completed_at>=? AND t.completed_at<?".into());
            values.extend([filter.period_start.into(), filter.period_end.into()]);
        }
        "progress" => {
            clauses.push("EXISTS (SELECT 1 FROM task_updates u WHERE u.task_id=t.id AND u.kind='progress' AND u.created_at>=? AND u.created_at<?)".into());
            values.extend([filter.period_start.into(), filter.period_end.into()]);
        }
        _ => return Err(AppError::validation("未知的交接候选类型")),
    }
    if let Some(search) = filter
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        clauses.push("(t.title LIKE ? ESCAPE '\\' OR IFNULL(t.description,'') LIKE ? ESCAPE '\\' OR IFNULL(t.department,'') LIKE ? ESCAPE '\\' OR EXISTS (SELECT 1 FROM task_updates u WHERE u.task_id=t.id AND u.content LIKE ? ESCAPE '\\'))".into());
        let like = format!("%{}%", escape_like(search));
        for _ in 0..4 {
            values.push(like.clone().into());
        }
    }
    let where_sql = clauses.join(" AND ");
    let total = conn.query_row(
        &format!("SELECT COUNT(*) FROM tasks t WHERE {where_sql}"),
        params_from_iter(values.iter()),
        |r| r.get(0),
    )?;
    let limit = filter.limit.unwrap_or(50).clamp(1, 2000);
    let offset = filter.offset.unwrap_or(0).max(0);
    values.extend([Value::Integer(limit), Value::Integer(offset)]);
    let mut stmt = conn.prepare(&format!("SELECT t.id FROM tasks t WHERE {where_sql} ORDER BY t.priority ASC, COALESCE(t.recorded_at,t.created_at) DESC, t.id ASC LIMIT ? OFFSET ?"))?;
    let ids = stmt
        .query_map(params_from_iter(values.iter()), |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let items = ids
        .iter()
        .map(|id| tasks::get_required(conn, id))
        .collect::<AppResult<Vec<_>>>()?;
    Ok(TaskPage {
        items,
        total,
        limit,
        offset,
    })
}

pub fn save(conn: &Connection, input: &HandoverSaveInput) -> AppResult<HandoverDetail> {
    period(input.period_start, input.period_end)?;
    let title = input.title.trim();
    if title.is_empty() || title.chars().count() > 200 {
        return Err(AppError::validation("交接标题须为 1 至 200 个字符"));
    }
    if input.items.len() > 10_000 {
        return Err(AppError::validation("单份交接最多包含 10000 个事项"));
    }
    let shift_from = optional_text(&input.shift_from, 100, "交班人")?;
    let shift_to = optional_text(&input.shift_to, 100, "接班人")?;
    let note = optional_text(&input.note, 20_000, "交接备注")?;
    let tx = conn.unchecked_transaction()?;
    let id = input
        .id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    if input.id.is_some() && header(&tx, &id)?.status != "draft" {
        return Err(AppError::validation("已定稿交接不可修改，请复制为新草稿"));
    }
    let mut seen = HashSet::new();
    let mut prepared = Vec::new();
    for item in &input.items {
        if !SECTIONS.iter().any(|(key, _)| *key == item.section) {
            return Err(AppError::validation("未知的交接分区"));
        }
        if !seen.insert(item.task_id.as_str()) {
            return Err(AppError::validation("同一事项只能加入一个主分区"));
        }
        let item_note = optional_text(&item.note, 20_000, "事项交接备注")?;
        let snapshot = draft_snapshot(
            &tx,
            &item.task_id,
            &item.selected_update_ids,
            input.period_start,
            input.period_end,
            true,
        )?;
        let selected_ids = snapshot
            .updates
            .iter()
            .map(|u| u.id.clone())
            .collect::<Vec<_>>();
        prepared.push((item, item_note, json(&selected_ids)?, json(&snapshot)?));
    }
    let now = now_ms();
    let date = Local
        .timestamp_millis_opt(input.period_start)
        .single()
        .ok_or_else(|| AppError::validation("无效的开始时间"))?
        .format("%Y-%m-%d")
        .to_string();
    if input.id.is_some() {
        tx.execute("UPDATE handovers SET title=?, handover_date=?, shift_from=?, shift_to=?, note=?, period_start=?, period_end=?, updated_at=? WHERE id=? AND status='draft'", params![title, date, shift_from, shift_to, note, input.period_start, input.period_end, now, id])?;
        tx.execute("DELETE FROM handover_tasks WHERE handover_id=?", [&id])?;
    } else {
        tx.execute("INSERT INTO handovers(id,title,handover_date,shift_from,shift_to,note,period_start,period_end,status,snapshot_version,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,'draft',1,?,?)", params![id,title,date,shift_from,shift_to,note,input.period_start,input.period_end,now,now])?;
    }
    for (index, (item, note, selected_ids, snapshot)) in prepared.iter().enumerate() {
        tx.execute("INSERT INTO handover_tasks(id,handover_id,task_id,section,note,sort_order,selected_update_ids_json,snapshot_json) VALUES(?,?,?,?,?,?,?,?)", params![Uuid::new_v4().to_string(),id,item.task_id,item.section,note,index as i64,selected_ids,snapshot])?;
    }
    let result = get(&tx, &id)?;
    tx.commit()?;
    Ok(result)
}

pub fn finalize(conn: &Connection, id: &str) -> AppResult<HandoverDetail> {
    let tx = conn.unchecked_transaction()?;
    let detail = get(&tx, id)?;
    if detail.handover.status == "finalized" {
        return Ok(detail);
    }
    for item in &detail.items {
        let task_id = item.task_id.as_deref().ok_or_else(|| {
            AppError::validation("草稿包含已永久删除的事项，请先移除失效条目再定稿")
        })?;
        let snapshot = draft_snapshot(
            &tx,
            task_id,
            &item.selected_update_ids,
            detail.handover.period_start,
            detail.handover.period_end,
            true,
        )?;
        tx.execute(
            "UPDATE handover_tasks SET snapshot_json=? WHERE id=? AND handover_id=?",
            params![json(&snapshot)?, item.id, id],
        )?;
    }
    let now = now_ms();
    tx.execute("UPDATE handovers SET status='finalized',finalized_at=?,updated_at=?,snapshot_version=1 WHERE id=? AND status='draft'", params![now,now,id])?;
    let result = get(&tx, id)?;
    tx.commit()?;
    Ok(result)
}

pub fn duplicate(conn: &Connection, id: &str) -> AppResult<HandoverDetail> {
    let source = get(conn, id)?;
    let mut items = Vec::new();
    for item in source.items {
        let task_id = item.task_id.ok_or_else(|| {
            AppError::validation(
                "此交接包含已永久删除的事项，无法复制为实时草稿；原定稿仍可预览和导出",
            )
        })?;
        // 复制时保留明确选项；历史进展已被删改或移出区间时明确报错。
        items.push(HandoverItemInput {
            task_id,
            section: item.section,
            note: item.note,
            selected_update_ids: item.selected_update_ids,
        });
    }
    let source = source.handover;
    let suffix = "（副本）";
    let title = format!(
        "{}{suffix}",
        source
            .title
            .chars()
            .take(200 - suffix.chars().count())
            .collect::<String>()
    );
    save(
        conn,
        &HandoverSaveInput {
            id: None,
            title,
            period_start: source.period_start,
            period_end: source.period_end,
            shift_from: source.shift_from,
            shift_to: source.shift_to,
            note: source.note,
            items,
        },
    )
}

pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    let tx = conn.unchecked_transaction()?;
    if header(&tx, id)?.status != "draft" {
        return Err(AppError::validation("仅可删除草稿；已定稿交接必须保留历史"));
    }
    tx.execute("DELETE FROM handovers WHERE id=? AND status='draft'", [id])?;
    tx.commit()?;
    Ok(())
}
