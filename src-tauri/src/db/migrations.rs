//! 数据库迁移。
//!
//! 每个迁移文件都是幂等的 SQL 脚本，执行时包裹在事务中，成功后写入
//! `PRAGMA user_version`。升级应用时只需在 `MIGRATIONS` 末尾追加新版本。

use rusqlite::Connection;

use crate::error::AppResult;

pub struct Migration {
    pub version: i64,
    pub name: &'static str,
    pub sql: &'static str,
}

pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "init",
        sql: include_str!("../../migrations/001_init.sql"),
    },
    Migration {
        version: 2,
        name: "department_and_recorded_at",
        sql: include_str!("../../migrations/002_department_and_recorded_at.sql"),
    },
    Migration {
        version: 3,
        name: "handover_snapshots",
        sql: include_str!("../../migrations/003_handover_snapshots.sql"),
    },
];

pub fn current_version(conn: &Connection) -> AppResult<i64> {
    Ok(conn.query_row("PRAGMA user_version", [], |row| row.get(0))?)
}

/// 依次应用尚未执行的迁移，返回迁移后的 schema 版本。
pub fn apply(conn: &mut Connection) -> AppResult<i64> {
    let from = current_version(conn)?;
    let mut applied = from;

    for migration in MIGRATIONS {
        if migration.version <= from {
            continue;
        }

        let tx = conn.transaction()?;
        tx.execute_batch(migration.sql)?;
        // PRAGMA 不支持参数绑定，只能拼接常量；version 来自编译期常量，无注入风险。
        tx.execute_batch(&format!("PRAGMA user_version = {};", migration.version))?;
        tx.commit()?;

        applied = migration.version;
        eprintln!(
            "[shiftnote] 已应用数据库迁移 v{} ({})",
            migration.version, migration.name
        );
    }

    Ok(applied)
}

/// 最新代码期望的 schema 版本，用于启动时的兼容性提示。
pub fn latest_version() -> i64 {
    MIGRATIONS.last().map(|m| m.version).unwrap_or(0)
}
