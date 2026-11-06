//! 用户配置（settings 表）读写。

use std::collections::HashMap;

use rusqlite::{params, Connection};

use crate::db::now_ms;
use crate::error::AppResult;

pub fn get(conn: &Connection, key: &str) -> AppResult<Option<String>> {
    let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?")?;
    let mut rows = stmt.query_map(params![key], |row| row.get::<_, String>(0))?;
    match rows.next() {
        Some(value) => Ok(Some(value?)),
        None => Ok(None),
    }
}

pub fn get_bool(conn: &Connection, key: &str, fallback: bool) -> AppResult<bool> {
    Ok(get(conn, key)?
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(fallback))
}

pub fn set(conn: &Connection, key: &str, value: &str) -> AppResult<()> {
    conn.execute(
        "INSERT INTO settings (key, value, updated_at) VALUES (?, ?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![key, value, now_ms()],
    )?;
    Ok(())
}

pub fn all(conn: &Connection) -> AppResult<HashMap<String, String>> {
    let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut map = HashMap::new();
    for row in rows {
        let (k, v) = row?;
        map.insert(k, v);
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    #[test]
    fn upserts_setting_values() {
        let db = Db::open_in_memory().unwrap();
        db.with_conn(|conn| {
            set(conn, "theme", "light")?;
            set(conn, "theme", "dark")?;
            assert_eq!(get(conn, "theme")?.as_deref(), Some("dark"));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn reads_boolean_flags() {
        let db = Db::open_in_memory().unwrap();
        db.with_conn(|conn| {
            set(conn, "close_to_tray", "1")?;
            assert!(get_bool(conn, "close_to_tray", false)?);
            assert!(get_bool(conn, "missing_key", true)?);
            Ok(())
        })
        .unwrap();
    }
}
