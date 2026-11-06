//! 不可信备份的结构白名单和资源边界；不得先迁移再检查结构。

use std::ops::{Deref, DerefMut};
use std::path::Path;
use std::time::{Duration, Instant};

use rusqlite::config::DbConfig;
use rusqlite::limits::Limit;
use rusqlite::types::ValueRef;
use rusqlite::Connection;

use super::{handovers, migrations, tasks, updates};
use crate::error::{AppError, AppResult};

/// 这些限制只用于导入，不降低在线业务连接的日常容量。
#[derive(Clone, Copy)]
pub(super) struct RestoreLimits {
    pub max_file_bytes: u64,
    pub max_rows_per_table: usize,
    pub max_total_rows: usize,
    pub max_value_bytes: usize,
    pub max_snapshot_bytes: usize,
    pub max_total_snapshot_bytes: usize,
    pub max_selection_bytes: usize,
    pub max_json_items: usize,
    pub max_vm_steps: u64,
    pub max_duration: Duration,
}

impl Default for RestoreLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: 512 * 1024 * 1024,
            max_rows_per_table: 100_000,
            max_total_rows: 500_000,
            max_value_bytes: 8 * 1024 * 1024,
            max_snapshot_bytes: 8 * 1024 * 1024,
            max_total_snapshot_bytes: 64 * 1024 * 1024,
            max_selection_bytes: 1024 * 1024,
            max_json_items: 10_000,
            max_vm_steps: 100_000_000,
            max_duration: Duration::from_secs(30),
        }
    }
}

pub(super) fn harden_connection(conn: &Connection) -> AppResult<()> {
    conn.set_db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)?;
    conn.execute_batch("PRAGMA trusted_schema=OFF; PRAGMA foreign_keys=ON;")?;
    Ok(())
}

/// rusqlite 的进度回调需要可变连接；离开作用域后恢复限制。
pub(super) struct SqlBudget<'a> {
    conn: &'a mut Connection,
    previous: Vec<(Limit, i32)>,
}

impl<'a> SqlBudget<'a> {
    pub(super) fn new(conn: &'a mut Connection, limits: &RestoreLimits) -> AppResult<Self> {
        if limits.max_vm_steps == 0 || limits.max_duration.is_zero() {
            return Err(AppError::validation("恢复执行预算已用尽"));
        }
        let mut budget = Self {
            conn,
            previous: Vec::new(),
        };
        for (kind, value) in [
            (Limit::SQLITE_LIMIT_LENGTH, 16 * 1024 * 1024),
            (Limit::SQLITE_LIMIT_SQL_LENGTH, 1024 * 1024),
            (Limit::SQLITE_LIMIT_COLUMN, 128),
            (Limit::SQLITE_LIMIT_EXPR_DEPTH, 100),
            (Limit::SQLITE_LIMIT_COMPOUND_SELECT, 50),
            (Limit::SQLITE_LIMIT_VDBE_OP, 100_000),
            (Limit::SQLITE_LIMIT_ATTACHED, 0),
            (Limit::SQLITE_LIMIT_VARIABLE_NUMBER, 1000),
            (Limit::SQLITE_LIMIT_WORKER_THREADS, 0),
        ] {
            let current = budget.conn.limit(kind)?;
            let previous = budget.conn.set_limit(kind, current.min(value))?;
            budget.previous.push((kind, previous));
        }
        let interval = limits.max_vm_steps.clamp(1, 1000) as i32;
        let mut remaining = limits.max_vm_steps;
        let deadline = Instant::now() + limits.max_duration;
        budget.conn.progress_handler(
            interval,
            Some(move || {
                if Instant::now() >= deadline || remaining < interval as u64 {
                    return true;
                }
                remaining -= interval as u64;
                false
            }),
        )?;
        Ok(budget)
    }
}

impl Deref for SqlBudget<'_> {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        &self.conn
    }
}

impl DerefMut for SqlBudget<'_> {
    fn deref_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }
}

impl Drop for SqlBudget<'_> {
    fn drop(&mut self) {
        let _ = self.conn.progress_handler(0, None::<fn() -> bool>);
        for &(kind, previous) in &self.previous {
            let _ = self.conn.set_limit(kind, previous);
        }
    }
}

pub(super) fn check_file_size(path: &Path, limits: &RestoreLimits) -> AppResult<()> {
    let mut total = 0_u64;
    for candidate in [
        path.to_path_buf(),
        super::sidecar_path(path, "-wal"),
        super::sidecar_path(path, "-journal"),
    ] {
        match std::fs::metadata(&candidate) {
            Ok(metadata) if metadata.is_file() => {
                total = total.saturating_add(metadata.len());
            }
            Ok(_) => return Err(AppError::validation("备份及其日志必须是普通文件")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        }
    }
    if total > limits.max_file_bytes {
        return Err(AppError::validation(
            "备份文件及日志合计超过恢复大小限制（默认 512 MiB）",
        ));
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
struct SchemaObject {
    kind: String,
    name: String,
    table: String,
    sql: Option<Vec<String>>,
}

/// 按 SQL token 比较，忽略注释、空白和非引号标识符大小写；不忽略字面量、
/// 约束、排序方向、部分索引条件或任何其他语法。
fn sql_tokens(sql: &str) -> AppResult<Vec<String>> {
    let bytes = sql.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if bytes[i..].starts_with(b"--") {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if bytes[i..].starts_with(b"/*") {
            i += 2;
            while i + 1 < bytes.len() && !bytes[i..].starts_with(b"*/") {
                i += 1;
            }
            if i + 1 >= bytes.len() {
                return Err(AppError::validation("备份结构含未闭合注释"));
            }
            i += 2;
            continue;
        }
        if matches!(bytes[i], b'\'' | b'"' | b'`' | b'[') {
            let end = if bytes[i] == b'[' { b']' } else { bytes[i] };
            i += 1;
            loop {
                if i >= bytes.len() {
                    return Err(AppError::validation("备份结构含未闭合引号"));
                }
                if bytes[i] == end {
                    i += 1;
                    if end != b']' && bytes.get(i) == Some(&end) {
                        i += 1;
                        continue;
                    }
                    break;
                }
                i += 1;
            }
            tokens.push(sql[start..i].to_owned());
        } else if bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] >= 128 {
            i += 1;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric()
                    || matches!(bytes[i], b'_' | b'$')
                    || bytes[i] >= 128)
            {
                i += 1;
            }
            tokens.push(sql[start..i].to_ascii_lowercase());
        } else {
            i += 1;
            tokens.push(sql[start..i].to_owned());
        }
    }
    if tokens.last().is_some_and(|token| token == ";") {
        tokens.pop();
    }
    Ok(tokens)
}

fn schema_objects(conn: &Connection) -> AppResult<Vec<SchemaObject>> {
    let mut stmt = conn
        .prepare("SELECT type,name,tbl_name,sql FROM sqlite_schema ORDER BY type,name LIMIT 129")?;
    let mut rows = stmt.query([])?;
    let mut objects = Vec::new();
    while let Some(row) = rows.next()? {
        if objects.len() >= 128 {
            return Err(AppError::validation("备份结构对象过多"));
        }
        for column in 0..4 {
            if let ValueRef::Text(value) = row.get_ref(column)? {
                if value.len() > 64 * 1024 {
                    return Err(AppError::validation("备份结构定义过长"));
                }
            }
        }
        let kind: String = row.get(0)?;
        if kind != "table" && kind != "index" {
            return Err(AppError::validation("备份包含不允许的触发器或视图"));
        }
        let sql = optional_text(row.get_ref(3)?)?;
        objects.push(SchemaObject {
            kind,
            name: row.get(1)?,
            table: row.get(2)?,
            sql: sql.map(sql_tokens).transpose()?,
        });
    }
    Ok(objects)
}

fn reference_schema(version: i64) -> AppResult<Vec<SchemaObject>> {
    let conn = Connection::open_in_memory()?;
    harden_connection(&conn)?;
    for migration in migrations::MIGRATIONS
        .iter()
        .filter(|m| m.version <= version)
    {
        conn.execute_batch(migration.sql)?;
    }
    schema_objects(&conn)
}

fn text(value: ValueRef<'_>) -> AppResult<&str> {
    value
        .as_str()
        .map_err(|_| AppError::validation("备份包含不兼容的文本字段类型"))
}

fn optional_text(value: ValueRef<'_>) -> AppResult<Option<&str>> {
    match value {
        ValueRef::Null => Ok(None),
        value => text(value).map(Some),
    }
}

fn validate_schema(conn: &Connection, version: i64) -> AppResult<Vec<String>> {
    if !migrations::MIGRATIONS
        .iter()
        .any(|migration| migration.version == version)
    {
        return Err(AppError::validation(
            "备份结构版本不受支持，请使用匹配版本的 ShiftNote",
        ));
    }
    let expected = reference_schema(version)?;
    if schema_objects(conn)? != expected {
        return Err(AppError::validation(
            "备份结构与该版本不一致：表、约束或索引被修改，或含额外对象",
        ));
    }
    Ok(expected
        .into_iter()
        .filter(|object| object.kind == "table")
        .map(|object| object.name)
        .collect())
}

fn check_resources(conn: &Connection, tables: &[String], limits: &RestoreLimits) -> AppResult<()> {
    let pages: i64 = conn.query_row("PRAGMA page_count", [], |r| r.get(0))?;
    let page_size: i64 = conn.query_row("PRAGMA page_size", [], |r| r.get(0))?;
    if pages < 0
        || page_size <= 0
        || (pages as u64).saturating_mul(page_size as u64) > limits.max_file_bytes
    {
        return Err(AppError::validation("备份逻辑大小超过恢复限制"));
    }
    let mut total_rows = 0_usize;
    let mut snapshot_bytes = 0_usize;
    for table in tables {
        let mut declarations =
            conn.prepare("SELECT type,\"notnull\",pk FROM pragma_table_info(?1) ORDER BY cid")?;
        let declarations = declarations
            .query_map([table], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)? != 0 || row.get::<_, i64>(2)? != 0,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        // 名称来自编译期迁移生成且已完全匹配的结构，而非未校验的外部输入。
        let quoted = format!("\"{}\"", table.replace('"', "\"\""));
        let mut stmt = conn.prepare(&format!("SELECT * FROM {quoted} LIMIT ?1"))?;
        let columns = stmt
            .column_names()
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let mut rows = stmt.query([limits.max_rows_per_table.saturating_add(1) as i64])?;
        let mut count = 0;
        while let Some(row) = rows.next()? {
            count += 1;
            total_rows += 1;
            if count > limits.max_rows_per_table || total_rows > limits.max_total_rows {
                return Err(AppError::validation("备份记录数量超过恢复限制"));
            }
            for (column, name) in columns.iter().enumerate() {
                let value = row.get_ref(column)?;
                let (declared, required) = &declarations[column];
                let valid_type = match value {
                    ValueRef::Null => !required,
                    ValueRef::Integer(_) => declared.eq_ignore_ascii_case("INTEGER"),
                    ValueRef::Text(bytes) => {
                        declared.eq_ignore_ascii_case("TEXT") && std::str::from_utf8(bytes).is_ok()
                    }
                    _ => false,
                };
                if !valid_type {
                    return Err(AppError::validation(format!(
                        "备份字段类型不兼容：{table}.{name}"
                    )));
                }
                let bytes = match value {
                    ValueRef::Text(bytes) | ValueRef::Blob(bytes) => bytes.len(),
                    _ => 0,
                };
                if bytes > limits.max_value_bytes {
                    return Err(AppError::validation("备份字段超过恢复长度限制"));
                }
                if table == "handover_tasks" && name == "snapshot_json" {
                    snapshot_bytes = snapshot_bytes.saturating_add(bytes);
                    if bytes > limits.max_snapshot_bytes
                        || snapshot_bytes > limits.max_total_snapshot_bytes
                    {
                        return Err(AppError::validation("交接快照超过恢复字节限制"));
                    }
                }
                if table == "handover_tasks"
                    && name == "selected_update_ids_json"
                    && bytes > limits.max_selection_bytes
                {
                    return Err(AppError::validation("交接进展选择超过恢复字节限制"));
                }
            }
        }
    }
    Ok(())
}

/// 必须在迁移或任何业务查询之前调用，适用于文件库和解密后的内存库。
/// 调用方为同一连接保持 SqlBudget，迁移后还必须调用 validate_database。
pub(super) fn inspect_backup(conn: &Connection, limits: &RestoreLimits) -> AppResult<i64> {
    harden_connection(conn)?;
    let version = migrations::current_version(conn)?;
    let tables = validate_schema(conn, version)?;
    check_resources(conn, &tables, limits)?;
    let check: String = conn.query_row("PRAGMA quick_check(1)", [], |row| row.get(0))?;
    if !check.eq_ignore_ascii_case("ok") {
        return Err(AppError::validation("备份文件完整性检查失败"));
    }
    Ok(version)
}

pub(super) fn validate_database(
    conn: &Connection,
    limits: &RestoreLimits,
) -> AppResult<(i64, i64, i64)> {
    let version = inspect_backup(conn, limits)?;
    if version != migrations::latest_version() {
        return Err(AppError::validation("备份尚未迁移到当前结构版本"));
    }
    let mut foreign_keys = conn.prepare("PRAGMA foreign_key_check")?;
    if foreign_keys.query([])?.next()?.is_some() {
        return Err(AppError::validation("备份存在悬挂外键"));
    }
    // 有界的真实仓储查询验证基本字段映射，不整批收集交接单和快照。
    tasks::page(conn, &Default::default())?;
    tasks::counts(conn)?;
    updates::list(conn, "__restore_validation__")?;
    handovers::list(conn, &Default::default())?;
    // settings 的流式类型检查避免 all() 把整个配置表复制到内存。
    let mut stmt = conn.prepare("SELECT key,value FROM settings")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let _: (&str, &str) = (text(row.get_ref(0)?)?, text(row.get_ref(1)?)?);
    }
    let unsupported: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM handovers WHERE snapshot_version<>1)",
        [],
        |r| r.get(0),
    )?;
    if unsupported {
        return Err(AppError::validation("交接快照格式版本不受支持"));
    }
    let mut stmt = conn.prepare("SELECT h.status,ht.selected_update_ids_json,ht.snapshot_json FROM handover_tasks ht JOIN handovers h ON h.id=ht.handover_id")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let selection = text(row.get_ref(1)?)?;
        let ids: Vec<String> = serde_json::from_str(selection)
            .map_err(|e| AppError::validation(format!("交接进展选择无法读取：{e}")))?;
        if ids.len() > limits.max_json_items {
            return Err(AppError::validation("交接进展选择数量超过恢复限制"));
        }
        if let Some(json) = optional_text(row.get_ref(2)?)? {
            let snapshot: crate::models::TaskSnapshot = serde_json::from_str(json)
                .map_err(|e| AppError::validation(format!("交接快照无法读取：{e}")))?;
            if snapshot.updates.len() > limits.max_json_items {
                return Err(AppError::validation("交接快照进展数量超过恢复限制"));
            }
        } else if text(row.get_ref(0)?)? == "finalized" {
            return Err(AppError::validation("已定稿交接条目缺少快照"));
        }
    }
    let task_count = conn.query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get(0))?;
    let update_count = conn.query_row("SELECT COUNT(*) FROM task_updates", [], |row| row.get(0))?;
    Ok((version, task_count, update_count))
}
