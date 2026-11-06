# ShiftNote

> 轻量级交接班管理与事项跟踪工具 · Windows 10 / 11 · 可选本地口令加密

ShiftNote 用于记录事项、持续追加进展和整理交接记录，不要求账号或服务器。

[![CI](https://github.com/kmoshan/shiftnote/actions/workflows/ci.yml/badge.svg)](https://github.com/kmoshan/shiftnote/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/kmoshan/shiftnote?display_name=release)](https://github.com/kmoshan/shiftnote/releases/latest)

## 当前进度

本轮按根目录《ShiftNote-项目审查与分工计划-2026-11-06.md》完成第二、三阶段收尾与第五阶段实现。源码已接入真实 SQLite，生产界面没有模拟数据；浏览器测试的 IPC 替身只存在于测试目录。

0.2.1 将口令加密改为可选：首次打开可选「暂不加密，直接进入」，之后启动不再提示；需要时在设置中启用。跳过后数据库和备份以明文保存。启用时设置至少 12 个字符的口令，之后每次启动需解锁；忘记口令无法找回。旧数据会保留，迁移前请关闭旧版程序；加密文件不能用 0.1.0 打开。已有加密数据不能通过跳过绕过原口令。

已兼容上游 `2d67e1f` 的版本检测：解锁或跳过后按 12 小时间隔向 GitHub 查询公开 Release，设置页可手动重试。只提示并由用户打开下载页，不自动下载或安装，也不会提示降级到更低版本。请求包含应用版本和普通网络元数据，不上传事项、口令或备份；详见 [兼容检查](docs/UPSTREAM-UPDATE-COMPAT-2026-11-06.md)。

| 阶段 | 内容 | 接口 | UI | 自动测试 | Windows 桌面实测 |
| --- | --- | --- | --- | --- | --- |
| 1 | 基础架构、版本迁移、备份恢复 | 已有并加固 | 已有 | 通过 | 原版本有历史记录，本轮恢复窗口待复验 |
| 2 | CRUD、多选批量、分页、拖动排序、归档 | 完成 | 完成 | 通过 | 待执行 |
| 3 | 详情、进展、历史检索、记录日期区间、一致性 | 完成 | 完成 | 通过 | 待执行 |
| 4 | 周期提醒、通知、稍后提醒、休眠恢复 | 仅表结构与停止规则 | 未实现 | — | — |
| 5 | 交接草稿、候选分页、分区、快照定稿、导出 | 完成 | 完成 | 通过 | 原生对话框与文本编辑器待验收 |
| 6 | 托盘、浮窗、自启、全局快捷键 | 未实现 | 未实现 | — | — |
| 7 | 测试、打包与安装 | CI 已接入新增回归 | — | 见测试清单 | NSIS 已生成，安装升级待验证 |

完整验证范围与剩余验收见 [docs/TESTING.md](docs/TESTING.md)。没有把浏览器测试当作 Tauri 原生窗口或安装包验收。

## 已实现功能

- **事项**：快速添加、编辑、P0–P3、负责部门、自定义记录时间、复制、完成/取消完成、隐藏/取消隐藏、回收站恢复与永久删除。
- **进展**：追加、编辑、删除、行内展开历史、详情时间线与历史全文关键词搜索。草稿和编辑状态按事项保存，快速切换不会串写；修改旧进展后摘要、行内历史和详情同步。
- **分页与过滤**：每页 50/100/200/500 条，显示当前筛选总数和当前页数量，侧栏保留全局计数。记录日期支持起止日，结束日完整包含；今日视图固定今天，搜索时也忽略其他视图保存的日期。
- **批量**：独立选择框与完成按钮分开，「全选当前页」；完成/取消完成、隐藏/取消隐藏、优先级、回收站恢复、归档/取消归档、永久删除使用单一事务，失败保留选择。切页、搜索或过滤会清空选择，提交期间锁定范围，永久删除确认数量。
- **手动排序与归档**：无搜索、优先级或日期过滤时可拖动及跨页移动，后端按完整视图持久化顺序。仅已完成事项可归档，常规视图及全局搜索排除归档，取消完成自动取消归档。完成、隐藏、归档和移入回收站停止提醒，恢复不会自动启用。
- **交接班**：侧栏新建或从已选事项发起；按跨午夜班次查询新增、完成、有进展及当前待处理候选，独立分页，分区、备注和逐条进展选择；保存重开、预览、定稿、复制为新草稿。进展出现不代表事项完成。
- **历史与导出**：定稿冻结当时的标题、部门、优先级、状态、记录时间和选定进展；原事项改动或永久删除不改变定稿。Markdown/TXT 共用预览生成器，UTF-8 无 BOM、CRLF，时间明确标注 UTC。取消保存不报错，覆盖需确认，原子写入失败保留目标原文件。
- **数据与界面**：内存 SQLite，可选口令加密；v1/v2→v3 迁移、浅色/深色主题、Ctrl+N / Ctrl+F 窗口内快捷键。恢复先验证完整结构白名单、资源限额和关联，再在内存迁移验证；覆盖前生成快照，快照与新备份采用当前选择的明文或加密格式。错口令或损坏文件不会触发重建空库。

## 下载安装

GitHub 当前公开版本为 [v0.2.0](https://github.com/kmoshan/shiftnote/releases/tag/v0.2.0)，**不包含本轮新增功能与安全修复**。包含版本检测兼容改动的本地 0.2.1 产物目录为 `release/ShiftNote-0.2.1-update-compat-windows-x64/`，便携包为同名 `.zip`，附 SHA-256 校验文件；尚未发布到 GitHub Releases。

直接运行本地版本时，保留 `shiftnote.exe` 旁的 `shiftnote.portable` 文件；事项保存在同目录 `data/shiftnote.db`，WebView2 缓存保存在 `data/webview2/`。移动或备份便携版时请保留整个 `data` 目录。

`shiftnote.db` 在未启用加密时为普通 SQLite 文件，启用后为认证加密文件，扩展名为兼容路径而保留。「立即备份」在普通模式生成 `.db`，加密模式生成 `.snbak`；恢复加密备份需使用生成时的口令，恢复不会改变当前是否启用加密的选择。启用加密后，设置页提供「加密旧备份」，成功验证密文后才移除原文件，失败原件保留。你在其他目录保存过的副本和 Markdown/TXT 导出仍需自行保管；导出界面会明确提示明文。

安装版没有便携标记，数据默认位于 `%APPDATA%\com.shiftnote.desktop\`，卸载不会主动删除数据。非空 `SHIFTNOTE_DATA_DIR` 优先于便携标记，并同时指定数据库和 WebView2 数据目录。启动失败会显示具体错误及日志位置，不再只静默退出。

当前产物未签名，Windows SmartScreen 仍可能显示未知发布者提示；本轮先修复运行问题，未配置代码签名。

## 开发环境

| 组件 | 要求 |
| --- | --- |
| Node.js | 建议 22.22.2+ LTS，使用 npm 锁文件 |
| Rust | 1.89+ stable MSVC，依赖以 Cargo.lock 为准 |
| Microsoft C++ Build Tools | VS 2022，Desktop development with C++ 工作负载 |
| Windows SDK | Windows 10 / 11 SDK |
| WebView2 | Windows 10 / 11 通常已有运行时 |

在 “x64 Native Tools Command Prompt for VS 2022” 中确认 `cargo`、`rustc`、`cl` 和 `link` 可用。
也可在 PowerShell 中执行 `. .\scripts\env.ps1`，自动发现 MSVC 与 SDK；该脚本不再依赖原开发机的 D 盘路径。

```powershell
npm ci
npm run app:dev      # Vite + Tauri 窗口
npm run app:build    # exe 与 NSIS 安装包
npm run build        # 前端类型检查与生产构建
```

默认产物：`src-tauri/target/release/shiftnote.exe` 和 `src-tauri/target/release/bundle/nsis/`；设置 `CARGO_TARGET_DIR` 时位于该目录。

使用隔离数据验证桌面功能：

```powershell
$env:SHIFTNOTE_DATA_DIR = Join-Path $env:TEMP 'shiftnote-testdata'
npm run app:dev
```

## 测试

```powershell
npm run typecheck
npm test
npm run build
cargo test --locked --manifest-path src-tauri/Cargo.toml
npx --no-install playwright install chromium
npm run test:e2e
```

兼容更新功能后，前端单测 71 项、完整 MSVC Rust 测试 112 项通过；另手工执行 1 项真实 GitHub 联网测试通过。Rust 测试操作真实 SQLite、明文及加密文件与 Windows 文件锁，浏览器使用模拟 IPC。完整构建、浏览器回归与原生验收边界见 [测试清单](docs/TESTING.md)。

若本机已有 Edge，可在 PowerShell 设置 `$env:PLAYWRIGHT_BROWSER_CHANNEL='msedge'` 后运行浏览器测试。

## 数据与模块

- 数据库：`%APPDATA%\com.shiftnote.desktop\shiftnote.db`。
- 备份：设置 →「立即备份」；恢复：设置 →「从备份恢复」，覆盖前自动生成 `backups/pre-restore-*` 快照（普通模式 `.db`，加密模式 `.snbak`）。
- `src/components/`、`src/views/`：事项和交接界面；`src/stores/`：请求版本、草稿和写入编排；`src/services/api.ts`：唯一 IPC 出口。
- `src-tauri/src/commands/`：Tauri 命令；`db/`：查询、事务、解锁与恢复；`vault.rs`：口令派生、认证加密与原子保存；`export.rs`：统一生成与原子写文件；`migrations/`：追加版本迁移。
- [架构说明](docs/ARCHITECTURE.md)、[数据库设计](docs/DATABASE.md)、[测试清单](docs/TESTING.md)、[安全审查与修复](docs/SECURITY-REVIEW-2026-11-06.md)。

## 当前限制

1. 第四阶段提醒调度和第六阶段系统集成尚未实现。
2. 全选仅当前页；跨筛选全选、批量复制和相同进展追加属于后续扩展。
3. 复制为实时交接草稿时，如原事项或选定进展已删除，会明确拒绝；原定稿仍可预览和导出。
4. 完整原生交接流程、文件对话框、目标文本编辑器和安装升级后的数据保留尚未在本轮验收。
5. 静态加密保护保存的数据库与备份；解锁后的进程内存、系统分页和诊断转储不属于该保护范围。WebView2 自身可能产生微软运行时诊断流量，不能把本地业务存储理解为整台电脑永不联网。
