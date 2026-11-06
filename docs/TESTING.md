# ShiftNote 测试清单

> 记录于 Windows 10 22H2（19045）· x64 · WebView2 156 · 屏幕 2560×1600 @1.5x

## 一、自动化测试

### 1.1 Rust 单元测试

```powershell
. .\scripts\env.ps1
cd src-tauri
cargo test
```

最近一次结果：**26 passed；0 failed**（耗时 0.16s）。

| 用例 | 覆盖点 |
| --- | --- |
| `migrates_to_latest_version` | 迁移建表：tasks / task_updates / reminders / handovers / handover_tasks / categories / settings |
| `migrates_v1_database_and_backfills_department_and_recorded_at` | v1 旧库自动升级：按 assignee 回填部门、按 created_at 回填记录时间，旧数据不丢 |
| `day_bounds_are_ordered_and_cover_24h` | 本地“今天”边界计算（含夏令时容差） |
| `only_corruption_triggers_recovery` | 只有真损坏才重建；锁冲突/权限错误不误伤数据库 |
| `detects_corrupt_database_and_recovers` | 损坏的 .db 文件自动备份重建 |
| `restores_from_backup_and_keeps_pre_restore_snapshot` | 从备份恢复：覆盖生效、自动生成恢复前快照、恢复后可继续读写 |
| `rejects_backup_that_is_not_shiftnote` | 非法备份被拒绝，且不影响现有数据 |
| `create_and_read_task` | 创建事项、标题 trim、默认记录时间 |
| `stores_department_and_custom_recorded_at` | 负责部门与自定义记录时间可写入；空字符串清空部门 |
| `filters_by_recorded_date_not_created_date` | 日期筛选按“记录时间”生效，而非创建时间 |
| `sorts_by_recorded_time_desc` | 按记录时间排序 |
| `rejects_empty_title` | 空标题被拒绝 |
| `list_filters_by_priority_and_view` | 视图过滤 + 优先级排序 |
| `completing_task_disables_reminder` | 完成即关闭提醒并清空下次提醒时间 |
| `hiding_removes_from_default_list_but_keeps_progress` | 隐藏后进展与计数不丢 |
| `trash_restore_and_permanent_delete` | 回收站恢复、永久删除级联清理进展 |
| `search_matches_title_description_and_progress` | 搜索命中标题 / 描述 / 历史进展 |
| `search_wildcards_are_escaped` | `%` 被转义，不能匹配任意内容 |
| `completed_task_can_be_restored` | 取消完成恢复为未完成 |
| `duplicate_copies_fields_without_progress` | 复制只复制字段，不带进展 |
| `appends_never_overwrite_history` | 追加两条后历史完整、最新在前（同毫秒时按 rowid 保证顺序稳定） |
| `edit_keeps_created_at_and_sets_edited_at` | 编辑保留创建时间并写入修改时间 |
| `rejects_blank_content` | 空进展被拒绝 |
| `deleting_update_keeps_task` | 删除单条进展不影响事项 |
| `upserts_setting_values` / `reads_boolean_flags` | 配置写入与布尔读取 |

### 1.2 前端类型检查与构建

```powershell
npm run typecheck   # vue-tsc --noEmit
npm run build       # 产出 dist/
```

最近一次结果：通过。产物 `index.js 175.37 kB (gzip 63.29 kB)`、`index.css 29.09 kB (gzip 6.75 kB)`。

### 1.3 打包

```powershell
npm run app:build
```

最近一次结果：

```
Finished `release` profile [optimized] in 1m 46s
Built application at: D:\dev\target\release\shiftnote.exe
Finished 1 bundle at: D:\dev\target\release\bundle\nsis\ShiftNote_0.1.0_x64-setup.exe (1.67 MiB)
```

## 二、手工功能验证（已执行）

环境：`SHIFTNOTE_DATA_DIR=D:\dev\shiftnote-testdata` 隔离数据目录，
以 release 版 `shiftnote.exe` 运行，并用界面操作 + 直接查询 SQLite 双向核对。

| # | 操作 | 预期 | 实测 |
| --- | --- | --- | --- |
| 1 | 空库启动 | 显示空状态，侧栏计数全 0 | ✅ |
| 2 | 快速添加输入“生产服务器异常问题排查”并回车 | 列表立即出现该事项，计数 1 | ✅ |
| 3 | 查询数据库 | `tasks` 新增 1 行，UUID 主键，priority=2，status=open，UTF-8 中文完整（11 字符） | ✅ |
| 4 | 详情面板追加进展“初步判断为数据库连接池耗尽…” | 时间线出现 1 条，时间戳正确，`tasks.updated_at` 同步刷新 | ✅ |
| 5 | 勾选完成 | 标题划线、显示“已完成”、待处理 1→0、已完成 0→1 | ✅ |
| 6 | 查询数据库 | `status=done`、`completed_at` 非空、进展记录仍为 1 条 | ✅ |
| 7 | 搜索“连接池”（只存在于进展中） | 命中该已完成事项，副标题说明“含已完成 / 已隐藏” | ✅ |
| 8 | 结束进程后重新启动 | 事项、进展、完成状态全部保留 | ✅ |
| 9 | 内存占用 | 空闲约 22–33 MB | ✅ |

### 2.1 v0.2 字段调整与恢复功能（已执行）

环境：`SHIFTNOTE_DATA_DIR=D:\dev\shiftnote-v2` 隔离目录。

| # | 操作 | 预期 | 实测 |
| --- | --- | --- | --- |
| 10 | 打开「新建事项」 | 只显示 标题 / 优先级 / 描述 / **负责部门** / **记录时间** | ✅ 记录人、负责人、交接对象、预计完成均已移除 |
| 11 | 填写标题与部门并保存 | `department`、`recorded_at` 正确落库 | ✅ 部门=运维二部，recorded_at 取表单打开时间 |
| 12 | 详情面板 | 显示 记录时间 / 负责部门 / 创建时间 / 最近更新 | ✅ |
| 13 | 列表行 | 显示「记录 今天 09:00 · 部门 运维二部」 | ✅ |
| 14 | 「全部事项」与「今日事项」计数 | 今日只统计记录时间在今天的 | ✅ 全部 3 / 今日 2（其中一条记录时间为昨天） |
| 15 | 设置页「立即备份」 | 备份目录生成 `.db` | ✅ 124 KB |
| 16 | 直接插入第 4 条数据后「从备份恢复」 | 数据回到 3 条，并生成含 4 条的恢复前快照 | ✅ 见下方核对 |
| 17 | 恢复后界面 | 事项总数刷新为 3，并显示快照路径 | ✅ |
| 18 | 3 条进展的事项（收起态） | 只显示最新一条，右侧出现「展开 3 条」 | ✅ |
| 19 | 点击「展开 3 条」 | 就地展开全部 3 条，含时间、处理人、「最新」标记，按钮变「收起」 | ✅ 09:50 / 09:30 / 09:00 三条齐全 |
| 20 | 点击「收起」 | 回到只显示最新一条 | ✅ |
| 21 | 只有 1 条进展的事项 | 不显示展开按钮 | ✅ |

恢复前后核对：

```
LIVE                                任务=3  进展=3   （临时插入的第 4 条已消失）
pre-restore-20261105-152213.db      任务=4  进展=3   （覆盖前的自动快照）
shiftnote-backup-20261105-151937.db 任务=3  进展=3   （恢复来源）
```

> 说明：本机 GUI 自动化存在焦点抢占，原生日期面板与系统文件选择框无法稳定模拟。
> 日期筛选的查询逻辑由单元测试与「今日事项」计数间接验证；界面上的
> 「今天 / 昨天」快捷按钮与日期选择器建议手工点一次确认。

## 三、手工功能验证（待执行，随阶段推进）

### 3.1 事项

- [ ] 编辑标题 / 描述 / 优先级 / 负责部门 / 记录时间，保存后列表与详情同步
- [ ] 复制事项：新事项标题带“（副本）”，不带进展
- [ ] 隐藏事项：从全部事项消失，出现在已隐藏，进展保留
- [ ] 取消隐藏：回到全部事项
- [ ] 移入回收站 → 恢复 → 数据完整
- [ ] 回收站永久删除 → 事项与进展都消失（二次确认生效）
- [ ] 空标题保存被拒绝并提示
- [ ] 日期筛选：点「昨天」只显示昨天记录的事项；再点一次取消
- [ ] 日期选择器任选一天，列表与计数同步

### 3.2 进展

- [ ] 行内「追加进展」可用，保存后条目立即出现
- [ ] 编辑进展：出现“已编辑 + 时间”，创建时间不变
- [ ] 删除进展：二次确认；其余进展不受影响
- [ ] 正序 / 倒序切换正确
- [ ] 处理人写入后显示为标签

### 3.3 视图与搜索

- [ ] 今日 / 待处理 / 重点 / 已完成 / 已隐藏 / 回收站计数与内容一致
- [ ] 优先级筛选 P0–P3 与排序切换正确
- [ ] 搜索 `%`、`_` 不产生通配匹配
- [ ] 搜索负责人、交接对象可命中

### 3.4 主题与界面

- [ ] 默认浅色；切换深色后重启仍为深色
- [ ] Ctrl+N 打开新建；Ctrl+F 聚焦搜索；Esc 关闭弹窗
- [ ] 窗口缩放至最小尺寸（940×620 逻辑像素）布局不破

### 3.5 数据与异常

- [ ] 设置页显示数据库路径、大小、事项/进展数量、schema 版本
- [ ] 「立即备份」生成 `.db` 且可用 SQLite 打开
- [ ] 手动把 `shiftnote.db` 改成乱码后启动 → 自动备份为 `shiftnote.corrupt-*.db` 并重建
- [ ] 数据目录只读时启动 → 给出可读错误（待完善为对话框提示）

### 3.6 性能

- [ ] 1000 条事项下列表滚动流畅（`LIMIT 500` 分页待接入）
- [ ] 冷启动到可交互 < 2s

## 四、回归重点（每次改动后必跑）

1. `cargo test` 全绿。
2. `npm run typecheck` 无错误。
3. 新增事项 → 追加进展 → 完成 → 重启，四步数据一致。
4. 搜索历史进展仍可命中已完成事项。
