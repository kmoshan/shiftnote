use serde::{Deserialize, Serialize};

/// 优先级：0 = P0 紧急，1 = P1 重要，2 = P2 普通，3 = P3 低。
pub const PRIORITY_MIN: i64 = 0;
pub const PRIORITY_MAX: i64 = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub priority: i64,
    /// "open" | "done"
    pub status: String,
    pub completed_at: Option<i64>,
    pub hidden: bool,
    pub archived: bool,
    pub deleted_at: Option<i64>,
    pub category_id: Option<String>,
    /// 负责部门（旧版的「负责人 / 记录人 / 交接对象」已合并到此字段）
    pub department: Option<String>,
    /// 记录时间：可手工指定，列表按此日期筛选
    pub recorded_at: i64,
    pub sort_order: i64,
    pub created_at: i64,
    pub updated_at: i64,

    // 由查询联表/子查询得到，便于列表直接展示
    pub latest_update_content: Option<String>,
    pub latest_update_at: Option<i64>,
    pub update_count: i64,
    pub reminder_enabled: bool,
    pub next_remind_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskUpdate {
    pub id: String,
    pub task_id: String,
    pub content: String,
    pub author: Option<String>,
    /// "progress" | "system"
    pub kind: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub edited_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskDetail {
    pub task: Task,
    pub updates: Vec<TaskUpdate>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TaskFilter {
    /// all | today | open | focus | done | hidden | archived | trash
    pub view: Option<String>,
    pub priority: Option<i64>,
    pub search: Option<String>,
    /// priority | recorded | created | updated | manual
    pub sort: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    /// 记录时间范围（UTC 毫秒时间戳，左闭右开 [date_from, date_to)）
    pub date_from: Option<i64>,
    pub date_to: Option<i64>,
    pub category_id: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CreateTaskInput {
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub priority: Option<i64>,
    #[serde(default)]
    pub department: Option<String>,
    #[serde(default)]
    pub category_id: Option<String>,
    /// 记录时间（毫秒时间戳）；缺省时取当前时间
    #[serde(default)]
    pub recorded_at: Option<i64>,
}

/// 局部更新。字符串字段传入空字符串表示清空（写入 NULL）。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TaskPatch {
    pub title: Option<String>,
    pub description: Option<String>,
    pub priority: Option<i64>,
    pub department: Option<String>,
    pub category_id: Option<String>,
    /// 记录时间；传入 <= 0 表示不修改
    pub recorded_at: Option<i64>,
    pub sort_order: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub all: i64,
    pub today: i64,
    pub open: i64,
    pub focus: i64,
    pub done: i64,
    pub hidden: i64,
    pub archived: i64,
    pub trash: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub data_dir: String,
    pub db_path: String,
    pub schema_version: i64,
    /// 数据库损坏时自动备份出的旧文件路径，正常时为 None
    pub recovered_backup: Option<String>,
    pub encrypted: bool,
    pub plaintext_backup_count: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EncryptBackupsResult {
    pub encrypted: u32,
    pub remaining: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DbStats {
    pub path: String,
    pub size_bytes: u64,
    pub task_count: i64,
    pub update_count: i64,
    pub schema_version: i64,
    pub wal_enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub path: String,
    pub size_bytes: u64,
    pub created_at: i64,
}

/// 从备份恢复后的结果，供前端提示与刷新使用。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreSummary {
    pub source_path: String,
    /// 覆盖前自动生成的当前数据快照，可用于回退
    pub snapshot_path: String,
    pub task_count: i64,
    pub update_count: i64,
    pub schema_version: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskPage {
    pub items: Vec<Task>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskBatchInput {
    pub ids: Vec<String>,
    pub action: String,
    pub priority: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskBatchResult {
    pub requested: i64,
    pub changed: i64,
    pub unchanged: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskMoveInput {
    pub id: String,
    pub before_id: Option<String>,
    pub after_id: Option<String>,
    pub view: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Handover {
    pub id: String,
    pub title: String,
    pub handover_date: String,
    pub shift_from: Option<String>,
    pub shift_to: Option<String>,
    pub note: Option<String>,
    pub period_start: i64,
    pub period_end: i64,
    pub status: String,
    pub finalized_at: Option<i64>,
    pub snapshot_version: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSnapshot {
    pub task: Task,
    pub updates: Vec<TaskUpdate>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoverItem {
    pub id: String,
    pub task_id: Option<String>,
    pub section: String,
    pub note: Option<String>,
    pub sort_order: i64,
    pub selected_update_ids: Vec<String>,
    pub snapshot: TaskSnapshot,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoverDetail {
    pub handover: Handover,
    pub items: Vec<HandoverItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoverPage {
    pub items: Vec<Handover>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoverItemInput {
    pub task_id: String,
    pub section: String,
    pub note: Option<String>,
    #[serde(default)]
    pub selected_update_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoverSaveInput {
    pub id: Option<String>,
    pub title: String,
    pub period_start: i64,
    pub period_end: i64,
    pub shift_from: Option<String>,
    pub shift_to: Option<String>,
    pub note: Option<String>,
    #[serde(default)]
    pub items: Vec<HandoverItemInput>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HandoverFilter {
    pub search: Option<String>,
    pub status: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoverCandidateFilter {
    pub period_start: i64,
    pub period_end: i64,
    pub kind: String,
    pub search: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoverExportResult {
    pub path: String,
    pub bytes: u64,
}
