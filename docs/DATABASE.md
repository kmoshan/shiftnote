# 数据库设计

本地数据库文件（可选加密）：`%APPDATA%\com.shiftnote.desktop\shiftnote.db`
（可用环境变量 `SHIFTNOTE_DATA_DIR` 覆盖数据目录，便于便携使用或测试。）

- 引擎：SQLite（`rusqlite` + `bundled` 静态编译，无需用户安装），生产连接仅在内存中运行。
- 持久化：0.2.1 首次可跳过加密，以普通 SQLite 文件保存；`settings.security.setup_complete=1` 记住选择，之后直接进入。设置可随时启用加密，文件名保留 `.db`，内容改为 Argon2id / AES-256-GCM 认证加密封装。每次加密保存生成新随机 nonce；格式头部也参与认证。已有密文不能跳过原口令。
- 模式：`journal_mode=MEMORY`、`temp_store=MEMORY`、`foreign_keys=ON`；生产数据不写入明文 WAL、恢复暂存库或自动恢复快照。
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
| archived | INTEGER | 0/1，仅完成事项可归档；常规视图排除，取消完成自动取消归档 |
| deleted_at | INTEGER | 非空即在回收站 |
| category_id | TEXT FK | → categories(id) ON DELETE SET NULL |
| department | TEXT | 负责部门（v2 起替代负责人/记录人/交接对象） |
| recorded_at | INTEGER | 记录时间：可手工指定，列表按它筛选日期 |
| sort_order | INTEGER | 手动排序权重（步长 10） |
| created_at / updated_at | INTEGER NOT NULL | 创建 / 最近更新 |

> v2 迁移后，旧列 `assignee` / `recorder` / `handover_to` / `due_at` 仍保留在表中但不再读写，
> 以保留历史结构与数据；新代码统一使用 `department` 与 `recorded_at`。0.2.0 的加密文件不能直接由旧版本打开。

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

`handovers` 保存 `id`、`title`、`handover_date`（本地日期标签）、`period_start`、`period_end`
（UTC 毫秒，半开区间，支持跨午夜）、`shift_from`、`shift_to`、`note`、
`status`（draft/finalized）、`finalized_at`、`snapshot_version`、`created_at`、`updated_at`。

`handover_tasks` 使用独立 `id` 主键，`handover_id` 关联交接并级联删除，
`task_id` 可空并使用 `ON DELETE SET NULL`。同一交接中的实时 task_id 唯一；
`section` 为 `new / handled / pending / follow_up / focus`，另有 `note`、`sort_order`、
`selected_update_ids_json` 与 `snapshot_json`。

草稿读取实时事项，同时保留上次保存副本，以便原事项删除后仍能打开并明确移除失效条目。
草稿中已删除或移出班次的进展保留所选 ID，保存/定稿前必须纠正。定稿在一个事务内冻结完整事项
及选定进展，后续事项改动、进展删除或事项永久删除均不改变定稿；定稿只允许复制为新草稿。
缺失实时来源时复制明确失败，原定稿仍可读取和导出。

候选独立分页，排除隐藏/归档/回收站：记录时间找本班新增，完成时间找本班完成，进展创建时间找
本班有进展，当前待处理不受班次限制。预览/Markdown/TXT 共用同一生成器；导出使用标注 UTC 的时间。

### settings（用户配置）

`key` TEXT PK、`value` TEXT、`updated_at` INTEGER。`settings_set` 使用 UPSERT。

### categories（分类）

`id`、`name`（唯一）、`color`、`sort_order`、`created_at`、`updated_at`。

## 2. 迁移策略

- 脚本位于 `src-tauri/migrations/`，编译期通过 `include_str!` 内嵌。
- `PRAGMA user_version` 记录已应用版本；已选择普通模式的库启动时验证并载入，其他库先保持锁定。加密库通过口令认证及预检后，在内存中按序执行未应用脚本，每个脚本一个事务；镜像按所选模式保存成功后才开放业务访问。
- 新增结构变更时：追加下一版本 SQL，在 `db/migrations.rs` 的 `MIGRATIONS` 末尾登记，
  编写对应的仓储测试，不修改已发布的历史脚本。

### 已应用的迁移

| 版本 | 内容 |
| --- | --- |
| 001 | 初始结构（7 张表 + 索引） |
| 002 | 新增 `department`、`recorded_at`；旧数据按 `assignee → recorder → handover_to` 回填部门，按 `created_at` 回填记录时间 |
| 003 | 交接班时间、状态、快照版本；重建关联表，独立主键、可空实时引用、选定进展与自足快照；保留旧数据为草稿 |

## 3. 备份与恢复

- 手动备份：设置页「立即备份」→ `db_backup` 命令，在连接互斥锁内序列化内存库；普通模式保存明文，加密模式执行认证加密。
- 备份位置：`<数据目录>\backups\shiftnote-backup-<时间戳>-<UUID>`，普通模式扩展名 `.db`，加密模式 `.snbak`。文件可跨电脑复制，不依赖原电脑的系统账户或本机密钥。
- 同一 vault 的备份可使用当前会话密钥恢复；其他电脑或其他 vault 生成的备份需提供生成时的原口令。口令相同也不代表 vault 相同，因为各自使用独立随机 salt。
- **从备份恢复**：设置页选择 `.snbak` 或兼容的旧 `.db`，必要时输入原口令并确认覆盖。后端流程：
  1. 检查来源大小与格式。加密来源先验证认证标签，错误口令、篡改或损坏文件不会覆盖当前数据；旧 `.db` 只读打开。
  2. 先执行结构白名单和版本检查，再复制或解密到内存暂存库，在内存迁移并检查全部预期表、列、约束、索引、外键、核心查询及交接 JSON；拒绝额外触发器、视图、虚拟表等不受支持结构。
  3. 按当前模式自动生成快照 `backups\pre-restore-<时间戳>-<UUID>`（普通模式明文 `.db`，加密模式 `.snbak`）。
  4. 新镜像按当前模式通过同目录临时文件原子发布；成功后才交换当前内存连接。发布失败保留原会话与原文件，恢复流程由同一个连接互斥锁覆盖。恢复不会隐式启用或关闭加密，也不会重新弹首次选择。
  5. 恢复成功后前端使旧请求和缓存失效，再重新加载列表、主题与统计信息。
- 生产路径不会因错误口令、损坏文件或迁移失败而自动重建空库。设置页和启动流程明确报告错误，保留原文件供后续处理。

### 3.1 旧版数据迁移

- 检测到旧版明文主库时，可跳过口令直接进入普通模式；如选择加密，则设置并确认口令，同时明确勾选同意迁移。应先关闭使用旧库的实例。
- 迁移通过 SQLite 读取已验证的旧库，在内存暂存并完成迁移，再按所选模式保存原 `shiftnote.db`。普通模式也使用内存连接、校验、单实例锁与失败回滚；之后在设置中启用加密时，原子写入成功才切换会话。仅主库加密不会自动处理所有历史备份。
- 设置页「加密旧备份」逐个转换备份目录中的明文 `.db`。新 `.snbak` 经过解密校验后才移除对应明文来源；失败保留原件并显示剩余数量。不要把普通删除等同于安全擦除，其他位置的副本仍需自行处理。

### 3.2 资源边界与失败处理

- 生产数据库镜像上限 128 MiB，密文文件另加固定封装头及认证标签；旧库导入同时检查附属日志大小。
- 每表最多 100,000 行，总计最多 500,000 行；单个值和单条交接快照最多 8 MiB，交接快照累计最多 64 MiB，所选进展 ID JSON 最多 1 MiB，单个 JSON 集合最多 10,000 项。
- 每个 SQL 校验阶段最多 100,000,000 个 VM 步、30 秒；复制另设 30 秒预算，并限制 SQL 长度、列数、表达式深度及附加数据库等。整次解锁或恢复可能包含多个阶段，不承诺总耗时为 30 秒。
- 生产写入必须满足下次打开的校验边界。保存失败或业务闭包返回错误时，从上次成功保存的同模式镜像重载；若重载也失败，则撤销会话并锁定后端访问，防止继续使用未保存的状态。
- 这些描述对应实现约定，实际检查结果见 `TESTING.md`，不表示所有回归与 Windows 原生验证都已完成。

### 3.3 保护范围

- 静态加密保护关闭后的数据库和 `.snbak` 备份；解锁期间数据存在于进程内存，操作系统分页文件与崩溃转储不在此保护范围内。
- 口令不写入设置或日志，也没有找回接口；忘记口令将无法解密数据或备份。
- Markdown/TXT 是用于分享的明文导出，界面会提示；它们不具备 `.snbak` 的加密保护。
