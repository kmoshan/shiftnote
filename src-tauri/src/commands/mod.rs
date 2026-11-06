//! 命令层：暴露给前端的 Tauri 命令。
//!
//! 每个命令只做参数校验与调用编排，业务逻辑位于 `db` 模块。
//! 注意：`tauri::generate_handler!` 需要命令函数位于其定义模块中，
//! 因此注册时统一使用 `commands::<module>::<fn>` 的完整路径。

pub mod app;
pub mod settings;
pub mod tasks;
