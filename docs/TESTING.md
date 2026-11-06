# ShiftNote 测试与验收

更新：2026-11-06（Asia/Tokyo）。基线 `409b58f`，按根目录项目审查与分工计划实施。

## 本轮结果与边界

| 验证 | 结果 | 验证范围 |
| --- | --- | --- |
| 锁文件安装 `npm ci --no-audit --no-fund` | 通过，167 包 | 使用官方 SHA256 核验的隔离 Node 24.15.0，不修改全局 Node |
| `npm test` | 61 项通过 | Pinia 请求乱序、草稿、分页、批量、解锁门禁、旧备份入口、处理人迁移和真实 Vue 组件交互；IPC 为测试替身 |
| `npm run typecheck` / `npm run build` | 通过 | 前端类型与生产构建 |
| Playwright | 5 项通过 | Node 24.15.0 + 本机 Edge，真实页面与路由；IPC 为测试替身；TEMP 重定向到工作区 |
| 完整 MSVC Rust 测试 | 98 项通过 | `cargo test --locked --manifest-path src-tauri/Cargo.toml`；真实 SQLite、隔离文件、加密重开、跨口令备份、错误回滚、WAL 迁移、防写租约和恢复边界 |
| 完整 Tauri Release 编译 | 通过 | `npm run app:build`；Windows x64 程序、Tauri 命令宏与注册入口均已编译 |
| NSIS 打包 | 通过 | `ShiftNote_0.2.0_x64-setup.exe`；最终文件校验值见本地产物 `SHA256SUMS.txt` |
| 交付文件检查 | 通过 | EXE/安装包与编译输出 SHA-256 一致；便携 ZIP 仅含 EXE、便携标记、构建说明和校验文件，不含数据目录 |
| npm 安全公告 | 0 告警 | `npm audit --ignore-scripts`，包含开发依赖；不是对所有第三方源码的安全认证 |
| 原生窗口基本流程（上一轮 0.1 本地版） | 通过 | 曾在隔离目录加载 WebView2，经真实 IPC 新增、关闭重启保留；该记录不能替代本次口令解锁的原生端到端验收 |
| 完整原生交互 / 安装升级 | 待验证 | 完整交接、对话框、安装与升级仍需桌面验收 |

所有新增前端测试、后端模块内测试和浏览器流程已接入 `.github/workflows/ci.yml`。本轮未推送或触发远端 CI，因此不能把 CI 配置存在当成该提交的远端检查通过。

## 常规执行

```powershell
npm ci
npm test
npm run typecheck
npm run build
cargo test --locked --manifest-path src-tauri/Cargo.toml
npx --no-install playwright install chromium
npm run test:e2e
```

Windows 完整 Rust/Tauri 检查应在已安装 MSVC 与 Windows SDK 的 x64 Native Tools 终端运行。
PowerShell 可先执行 `. .\scripts\env.ps1`，自动发现已安装的 MSVC/SDK，并将本进程 TMP/TEMP 指向忽略的工作区构建目录。测试 Node 需符合 package.json 的 engines。

浏览器可使用已安装 Edge：

```powershell
$env:PLAYWRIGHT_BROWSER_CHANNEL = 'msedge'
npm run test:e2e
```

浏览器套件自启 Vite，不会修改生产代码的运行时检测；测试 IPC 替身仅在 `tests/e2e/workflows.spec.ts` 注入。截图和失败 trace 位于忽略的 `test-results/`。

此前缺少 MSVC 时曾用隔离 harness 验证 67 项业务测试；安装工具链后 0.1 本地实现通过 73 项。当前 0.2.0 已使用 VS 2022 Build Tools 和 Windows SDK 运行完整 Tauri crate 的 98 项 Rust 测试，全部通过。

本地产物整理在 `release/ShiftNote-0.2.0-windows-x64/`：`shiftnote.exe`、便携标记 `shiftnote.portable`、NSIS 安装包及 `SHA256SUMS.txt`。目录已在 Git 中忽略，未发布远端版本。生成命令为 `npm run app:build -- -- --locked`（最后的 `--locked` 传给 Cargo）。

新增安全回归覆盖完整历史 schema 白名单、额外对象/约束/索引拒绝、全部行字段类型检查、文件/记录/字段/快照/SQL 预算、旧版备份源不改动、口令校验与随机 nonce、篡改拒绝、单实例锁、加密备份原口令、恢复前加密快照、保存失败内存回滚、错误闭包不落盘、无法重载时锁定，以及 Windows 源租约与源变更拒绝。没有构造或运行破坏性利用载荷；跨电脑备份用本机两个独立目录和口令模拟，不是第二台物理机器测试。

### 启动故障复现与处理

原产物在本机点击运行后发生 `0xc0000409`，捕获 stderr 显示 Tauri 自动创建 WebView2 窗口时遇到 `os error 5`。独立 Rust 小程序也无法向本机系统 AppData/TEMP 新建目录，但可写工作区；不能把该现象仅归因于签名。

现在先初始化数据，再显式创建窗口；`SHIFTNOTE_DATA_DIR` 同时覆盖数据库与 WebView2。直接运行的本地产物使用显式 `.portable` 标记，写程序旁 `data/`；没有标记的安装版仍使用标准 AppData，不会静默切换数据库。无权限等启动错误会显示具体路径，追加日志并正常退出 1；实测默认 AppData 不可写时已不再直接 abort。日志依次尝试数据目录、TEMP/ShiftNote 和 exe 所在目录的 `logs/startup.log`。

上一轮 0.1 本地便携程序的同字节副本在 `audit.local/Portable QA with spaces/` 运行；未设置 `SHIFTNOTE_DATA_DIR` 或重定向 TEMP，工作目录也与 exe 所在目录不同。已验证页面加载、新增“便携重启验证 2026-11-06”、关闭并重开后仍显示同一事项。测试数据库未放入交付包。这是旧版在本机的便携运行验证，不代表新版口令流程、安装升级或另一台物理机器已验收。

本次尝试启动 0.2.0 隔离副本时，启动调用未返回，期间出现 SmartScreen 进程且未获得新版窗口，随后已取消该启动调用。未绕过系统安全提示，未将这次尝试计作原生启动通过。原有 0.1 程序及其数据未被关闭或改动。

未更改 Windows 安全设置或目录 ACL。当前没有代码签名证书，SmartScreen 提示仍属于未完成的发布签名工作。

## 自动回归对应验收

| 验收 | 覆盖 |
| --- | --- |
| R1 / T01 | A 输入→B 不出现→返回 A 保留；A 保存未结束时 B 新草稿不被清空；编辑状态按事项隔离 |
| R2 / T01 | 列表、详情的成功/失败/loading 版本保护；关闭、恢复使旧响应失效；连续切换 20 次、乱序成功与失败后只保留第 20 项 |
| R3 / T03 | 1,001 条相同排序值稳定遍历；相同搜索/优先级/记录日期条件的 total；删除最后一页后页码回退 |
| R4 / T02 | 共享进展缓存，编辑旧条目后行内历史刷新，摘要与详情使用最新数据 |
| R5 / T02 / T06 | 昨天→今日不带残留日期；今日搜索固定今天；返回其他视图恢复日期；跨年结束日包含整日，反向区间不发请求 |
| T04 / T05 | IDs 去重、上限、非法参数/不存在/状态不允许整批失败、幂等完成时间、事务中途失败回滚提醒、当前页全选、失败保留与提交锁定 |
| T10 | 仅完成可归档，常规视图排归档，取消完成自动取消归档；完整视图移动、跨页锚点与非法锚点；真实组件 drag 事件 |
| T07 / T08 | v1/v2 升级保留旧关联；跨午夜；候选 1,001 条独立分页；去重、分区、备注、逐条进展；失效进展和已删事项可打开纠正 |
| T09 | 定稿不可修改，原事项改动/删除不改变快照；事务失败回滚；晚到实时响应不覆盖定稿；预览与文件用同一文本生成器 |
| 导出 | 中文、换行、Markdown 特殊字符、五个空分区、稳定排序、UTF-8 无 BOM、CRLF；取消、拒绝覆盖、覆盖失败重试；真实 Windows 占用失败保留原文件 |
| R6 / T11 | 缺表、缺列、伪版本、外键悬挂、约束伪造、迁移失败、不可读交接 JSON、覆盖后验证失败、回滚失败快照路径、恢复锁覆盖写入和回滚 |

交接 UI 另覆盖：新建页 query 变化须确认、放弃修改后重开加载已保存内容、取消查看事项不触发详情、候选读取未结束时禁止保存/定稿、切记录清理加载状态、离开后旧响应失效。

## Windows 桌面验收（待执行）

必须使用隔离数据目录，不操作日常数据库：

```powershell
$env:SHIFTNOTE_DATA_DIR = Join-Path $env:TEMP 'shiftnote-acceptance'
npm run app:dev
```

- [x] 完整 MSVC `cargo test`：98 项通过。
- [x] 上一轮 0.1 本地版原生窗口启动和真实 IPC 新增事项写入隔离数据库。
- [x] 上一轮 0.1 本地版无数据环境变量、不同工作目录、包含空格的便携路径；新增事项关闭重开后保留。
- [ ] 0.2.0 原生窗口手工设置口令、解锁、重启及备份恢复全流程（核心存储已有真实 Rust 自动回归，UI 已有模拟 IPC 浏览器回归）。
- [ ] 1,001 条事项在原生窗口逐页访问，最后一条删除后页码有效，计数与真实数据库一致。
- [ ] 快速切换 20 次、跨事项草稿、旧进展编辑、今日和日期区间在原生 WebView2 中一致。
- [ ] 混合状态批量、失败选择保留、归档/取消归档和手动顺序重启后保持。
- [ ] 交接新建→候选→分区备注→预览→保存重开→定稿→导出完整操作。
- [ ] 定稿后编辑/删除原事项，历史详情和导出仍不变。
- [ ] 原生保存对话框取消、已有文件覆盖确认、只读目录和文件占用错误提示。
- [ ] 在目标 Windows 文本编辑器打开中文 Markdown/TXT，检查编码、换行、时间和特殊字符。
- [ ] 备份恢复成功/失败回滚后仍可读写，侧栏、详情、草稿、主题和 schema 版本同步。
- [ ] 940×620 最小窗口、浅色/深色、键盘操作；窗口内 Ctrl+N / Ctrl+F。
- [x] `npm run app:build` 生成 Windows x64 程序与 NSIS 安装包。
- [ ] 原版安装→产生数据→安装升级→数据保留；卸载不主动删除数据。

## 原版本历史验证

仓库原有 v0.1.0 记录：Windows 10 22H2 / WebView2 156 上 26 项 Rust 测试、前端构建和 NSIS 打包通过，执行过新增→追加进展→完成→重启，以及部门/记录时间调整、备份恢复和行内历史展开。上述属于旧版本记录，本轮新增交接和批量功能仍需完成前面的桌面清单。
