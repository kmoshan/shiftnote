# 数据库设计

数据库文件：`%APPDATA%\com.shiftnote.desktop\shiftnote.db`
（可用环境变量 `SHIFTNOTE_DATA_DIR` 覆盖数据目录，便于便携使用或测试。）

- 引擎：SQLite（`rusqlite` + `bundled` 静态编译，无需用户安装）
- 模式：WAL，`synchronous=NORMAL`，`foreign_keys=ON`，`busy_timeout=5000`
- 主键：业务数据统一使用 UUID v4（TEXT）
- 时间：Unix 毫秒时间戳（INTEGER，UTC），展示时转本地时区

## 1. 表结构

### tasks（事项主表）

| 列 | 类型 | 说明 |
| --- | --- | --- |
| id | TEXT PK | UUID |
| title | TEXT NOT NULL | 事项标题 |
| description | TEXT | 详细描述 |
| priority | INTEGER | 0=P0 紧急 … 3=P3 低，CHECK 约束 |
| status | TEXT | `open` / `done` |
| completed_at | INTEGER | 完成时间 |
| hidden | INTEGER | 0/1，从列表隐藏 |
| archived | INTEGER | 0/1，预留归档标记 |
| deleted_at | INTEGER | 非空即在回收站 |
| category_id | TEXT FK | → categories(id) ON DELETE SET NULL |
| department | TEXT | 负责部门（v2 起替代负责人/记录人/交接对象） |
| recorded_at | INTEGER | 记录时间：可手工指定，列表按它筛选日期 |
| sort_order | INTEGER | 手动排序权重（步长 10） |
| created_at / updated_at | INTEGER NOT NULL | 创建 / 最近更新 |

> v2 迁移后，旧列 `assignee` / `recorder` / `handover_to` / `due_at` 仍保留在表中但不再读写，
> 便于回退旧版本或手工取数；新代码统一使用 `department` 与 `recorded_at`。

索引：`(priority, sort_order)`、`(created_at)`、`(updated_at)`、`(recorded_at DESC)`、`(status, hidden, deleted_at)`、`(category_id)`、`(completed_at)`。

### task_updates（进展记录）

| 列 | 说明 |
| --- | --- |
| id | UUID 主键 |
| task_id | FK → tasks(id) ON DELETE CASCADE |
| content | 进展正文（≤4000 字符） |
| author | 处理人（选填） |
| kind | `progress` / `system`（预留系统记录） |
| created_at | 首次记录时间，**编辑时不变** |
| updated_at | 最近一次写入时间 |
| edited_at | 修改时间，`NULL` 表示未修改过 |

索引：`(task_id, created_at DESC)`、`(created_at DESC)`。

> 追加进展始终是 INSERT；编辑只更新 content/updated_at/edited_at，历史不被覆盖。

### reminders（提醒配置与状态）

| 列 | 说明 |
| --- | --- |
| task_id | 主键 + FK → tasks(id) ON DELETE CASCADE（一事一提醒配置） |
| enabled | 开关 |
| interval_minutes | 间隔分钟数 |
| first_remind_at | 首次提醒时间 |
| next_remind_at | 下次应触发时间（调度器依据，持久化） |
| last_notified_at | 上次通知时间（去重用） |
| snooze_until | “稍后提醒”截止时间 |
| remind_count | 已提醒次数 |

索引：`(enabled, next_remind_at)`、`(snooze_until)`。

> 本阶段仅建表并在完成/隐藏/删除时关闭提醒；调度器在阶段 4 实现。

### handovers / handover_tasks（交接班）

`handovers`：`id`、`handover_date`（YYYY-MM-DD 本地日期）、`shift_from`、`shift_to`、
`note`（特别注意事项）、`created_at`、`updated_at`。

`handover_tasks`：`(handover_id, task_id)` 复合主键，`section` 取值
`new / handled / pending / follow_up / focus`，另有 `note`、`sort_order`。

> 交接班只引用 `task_id`，不复制事项内容，避免多处更新不一致。
> 事项被永久删除时关联记录级联删除。

### settings（用户配置）

`key` TEXT PK、`value` TEXT、`updated_at` INTEGER。`settings_set` 使用 UPSERT。

### categories（分类）

`id`、`name`（唯一）、`color`、`sort_order`、`created_at`、`updated_at`。

## 2. 迁移策略

- 脚本位于 `src-tauri/migrations/`，编译期通过 `include_str!` 内嵌。
- `PRAGMA user_version` 记录已应用版本；启动时按序执行未应用的脚本，每个脚本一个事务。
- 新增结构变更时：追加 `003_xxx.sql`，在 `db/migrations.rs` 的 `MIGRATIONS` 末尾登记，
  编写对应的仓储测试，不修改已发布的历史脚本。

### 已应用的迁移

| 版本 | 内容 |
| --- | --- |
| 001 | 初始结构（7 张表 + 索引） |
| 002 | 新增 `department`、`recorded_at`；旧数据按 `assignee → recorder → handover_to` 回填部门，按 `created_at` 回填记录时间 |

## 3. 备份与恢复

- 手动备份：设置页「立即备份」→ `db_backup` 命令，使用 SQLite 在线备份 API
  （`Connection::backup`），运行中也能得到一致快照。
- 备份位置：`<数据目录>\backups\shiftnote-backup-<yyyyMMdd-HHmmss>.db`。
- **从备份恢复**：设置页「从备份恢复」→ 选择 `.db` 文件 → 二次确认。后端流程：
  1. 预检：`quick_check`、`user_version` 不得高于当前版本、必须包含 `tasks` 与 `task_updates`；
  2. 自动把当前数据快照到 `backups\pre-restore-<时间戳>.db`；
  3. 用 backup API 把备份内容覆盖到在线连接（无需重启应用）；
  4. 补齐缺失迁移；执行 `pragma_foreign_key_check`；
  5. 任一步失败自动用快照回滚，并把错误返回给界面。
- 恢复成功后前端会重新加载列表、主题与统计信息。
- 损坏自愈：启动自检失败时自动重命名为 `shiftnote.corrupt-<时间戳>.db` 并重建空库。
