# ShiftNote

> 轻量级交接班管理与事项跟踪工具 · Windows 10 / 11 · 数据完全本地保存

ShiftNote 面向个人、值班人员和中小部门：用最少的操作记录一件事，持续追加处理进展，
按优先级提醒，并在交接班时一键生成交接记录。它不是 OA / CRM，不要求登录，不依赖服务器。

[![CI](https://github.com/kmoshan/shiftnote/actions/workflows/ci.yml/badge.svg)](https://github.com/kmoshan/shiftnote/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/kmoshan/shiftnote?display_name=release)](https://github.com/kmoshan/shiftnote/releases/latest)
![Platform](https://img.shields.io/badge/platform-Windows%2010%20%7C%2011-0078D4)

### 下载安装

前往 [Releases](https://github.com/kmoshan/shiftnote/releases/latest) 下载最新的
`ShiftNote_*_x64-setup.exe`（约 2 MB），双击安装即可，无需管理员权限。
程序启动后数据保存在 `%APPDATA%\com.shiftnote.desktop\`，卸载不会主动删除数据。

安装后在「设置 → 关于」里可以**检测更新**：启动时会静默检查一次（12 小时内不重复），
有新版本会在顶部显示提示条，点击直达下载页。检查只读取 GitHub 上的公开版本号，
不会发送任何本地数据。

---

## 一、当前进度

| 阶段 | 内容 | 状态 |
| --- | --- | --- |
| 第一阶段 | 架构设计、项目初始化、数据库结构、可运行桌面版本 | ✅ 已完成 |
| 第二阶段 | 事项新增/编辑/删除、优先级、列表、完成勾选、隐藏恢复 | 🟡 主体已实现，批量操作待补 |
| 第三阶段 | 详情页、进展时间线、追加/编辑/删除进展、历史检索 | 🟡 主体已实现，日期范围筛选待补 |
| 第四阶段 | 周期提醒、系统通知、稍后提醒、休眠恢复 | ⬜ 未开始（表结构已就绪） |
| 第五阶段 | 交接班记录、关联事项、导出 Markdown/TXT | ⬜ 未开始（表结构已就绪） |
| 第六阶段 | 托盘、迷你浮窗、开机自启、全局快捷键 | ⬜ 未开始 |
| 第七阶段 | 完整测试、打包与安装说明 | 🟡 NSIS 打包已打通 |

> 已经实现的功能全部落在真实 SQLite 上，**没有任何前端模拟数据或占位按钮**。
> 尚未实现的模块不会出现在界面上。

### 本阶段已可用

- **事项**：快速添加（回车即存）、完整编辑、优先级 P0–P3、
  负责部门、记录时间（可手工指定）、复制、隐藏/取消隐藏、移入回收站/恢复/永久删除。
- **进展**：行内直接追加、详情页时间线、编辑（保留创建时间与修改时间）、删除（二次确认）；
  列表行默认只显示最新一条，点「展开 N 条」即可就地展开该事项的全部历史进展（按需加载）。
- **视图**：全部 / 今日 / 待处理 / 重点 / 已完成 / 已隐藏 / 回收站，带实时计数；
  列表支持**按记录日期筛选**（今天 / 昨天快捷项或任选一天）、优先级筛选与四种排序。
- **搜索**：标题、描述、负责部门 + **全部历史进展内容**，
  覆盖已完成与已隐藏事项，通配符自动转义。
- **数据**：UUID 主键、毫秒时间戳、WAL、外键级联、版本化迁移（v1→v2 自动升级）、
  损坏自愈、在线备份，以及**从备份一键恢复**（覆盖前自动快照、失败自动回滚）。
- **界面**：浅色/深色主题、三栏布局（侧边栏 / 列表 / 详情）、快捷键 Ctrl+N、Ctrl+F。
- **更新**：启动静默检查 + 设置页手动「检测更新」，有新版本时顶部提示条直达下载页；
  只读取 GitHub 公开版本号，直连失败会自动尝试系统代理。

---

## 二、技术栈

| 层 | 选型 |
| --- | --- |
| 桌面框架 | Tauri 2（WebView2，安装包体积远小于 Electron） |
| 前端 | Vue 3 + TypeScript + Vite 7 |
| 样式 | Tailwind CSS 4 + 自定义设计变量 |
| 状态 | Pinia |
| 图标 | Lucide（`@lucide/vue`） |
| 数据库 | SQLite（rusqlite `bundled`，无需用户安装） |
| 后端 | Rust（命令层 + 仓储层） |
| 打包 | Tauri CLI + NSIS（当前用户安装，免管理员） |

---

## 三、开发环境

### 3.1 必需组件

| 组件 | 版本要求 | 本机安装位置 |
| --- | --- | --- |
| Node.js | ≥ 20（本项目实测 24.14） | 系统已装 |
| Rust | ≥ 1.77（实测 1.99.1） | `D:\dev\rust`（`RUSTUP_HOME` / `CARGO_HOME`） |
| MSVC Build Tools | VS 2022 + VC++ x64 | `D:\dev\vsbt` |
| Windows SDK | 10.0.22621.0 | `C:\Program Files (x86)\Windows Kits\10` |
| WebView2 Runtime | Win10/11 通常已预装（实测 156.x） | 系统 |

### 3.2 本机已固化的环境变量

由于本机 C 盘空间紧张，工具链与构建产物都放在 D 盘，并已写入用户环境变量：

```
RUSTUP_HOME       = D:\dev\rust\rustup
CARGO_HOME        = D:\dev\rust\cargo
CARGO_TARGET_DIR  = D:\dev\target
INCLUDE / LIB     = MSVC + Windows SDK 的头文件与库目录
Path              += D:\dev\rust\cargo\bin; <MSVC>\bin\HostX64\x64
```

新开的终端会自动继承。若在未继承的会话中构建（例如 IDE 内置终端），
先加载项目脚本：

```powershell
. .\scripts\env.ps1
```

该脚本等价于执行 `vcvars64.bat`，为 Rust 提供 `link.exe` 与系统库。

### 3.3 从零安装（换一台机器时）

```powershell
# 1) Rust
winget install Rustlang.Rustup
rustup default stable-msvc

# 2) MSVC + Windows SDK（必须包含 VC++ x64 与 Windows SDK）
winget install Microsoft.VisualStudio.2022.BuildTools `
  --override "--quiet --norestart --nocache --add Microsoft.VisualStudio.Workload.VCTools --add Microsoft.VisualStudio.Component.Windows11SDK.22621"

# 3) 依赖
npm install
```

> 若 `cargo` 报 `linker link.exe not found`，说明当前终端没有 MSVC 环境，
> 请在 “x64 Native Tools Command Prompt for VS 2022” 中执行，或先运行 `scripts\env.ps1`。

---

## 四、启动与构建

```powershell
cd shiftnote

npm install            # 首次
npm run app:dev        # 开发模式：启动 Vite + Tauri 窗口（热更新）
npm run app:build      # 生产构建：生成 exe 与 NSIS 安装包
npm run build          # 仅构建前端（含 vue-tsc 类型检查）
npm run typecheck      # 仅类型检查
```

产物位置：

```
release\ShiftNote_<version>_x64-setup.exe        # 已复制到项目内，便于分发
D:\dev\target\release\shiftnote.exe             # 免安装可执行文件
D:\dev\target\release\bundle\nsis\ShiftNote_<version>_x64-setup.exe
```

> 安装包约 2.3 MiB，按当前用户安装（NSIS 会记住上次安装目录），无需管理员权限。
> SHA256（0.2.0）：`B281ABFE69E7A931F1389195D6ECA3485CEA81E6B0CD8046C58AF86E9814F4FB`

调试数据与正式数据相互隔离的技巧：

```powershell
$env:SHIFTNOTE_DATA_DIR = "D:\dev\shiftnote-data"
npm run app:dev
```

---

## 五、测试

### 5.1 Rust 单元测试（数据库与业务规则）

```powershell
. .\scripts\env.ps1
cd src-tauri
cargo test
```

覆盖点包括：迁移建表、标题校验、视图过滤、优先级排序、完成即停提醒、
隐藏后进展不丢失、回收站恢复、永久删除级联、搜索命中历史进展、通配符转义、
进展编辑保留创建时间、损坏数据库自愈等。

### 5.2 前端类型与构建检查

```powershell
npm run typecheck
npm run build
```

### 5.3 手工功能核对

启动 `npm run app:dev` 后按以下顺序验证（完整清单见 `docs/TESTING.md`）：

1. 顶部输入框输入标题 → 回车 → 列表立即出现该事项；关闭软件重开 → 仍在。
2. 行内「追加进展」写入两条 → 打开详情 → 时间线显示两条，最新在前。
3. 编辑第一条进展 → 条目出现“已编辑”，创建时间不变。
4. 勾选完成 → 标题划线、进入「已完成」、从「待处理」消失。
5. 取消完成 → 回到待处理。
6. 菜单「隐藏」→ 从全部事项消失，在「已隐藏事项」可见且进展仍在。
7. 菜单「移入回收站」→ 回收站可见；「恢复」→ 回到全部事项。
8. 搜索某条进展中的关键词 → 能命中对应事项。
9. 设置页「立即备份」→ 备份目录生成 `.db` 文件。

### 5.4 第一阶段实测结果

在 Windows 10 22H2（19045）/ x64 / WebView2 156 / 2560×1600@1.5x 环境下：

| 项目 | 结果 |
| --- | --- |
| `cargo test` | 26 passed / 0 failed || `npm run typecheck` | 通过 |
| `npm run build` | 175.35 kB JS（gzip 63.29 kB）、29.12 kB CSS |
| `npm run app:build` | 首次 1m46s、增量 1m00s；exe 4.14 MB，NSIS 1.67 MB |
| `npm run app:dev` | 正常启动，Vite HMR 可用 |
| 安装包 | 静默安装 → 运行 → 卸载 全流程通过，无残留 |
| 界面实测 | 新建 → 追加进展 → 完成 → 重启，数据库双向核对一致 |
| 内存占用 | 空闲 22–35 MB |

---

## 六、目录结构

```
shiftnote/
├── src/                          # 前端
│   ├── components/
│   │   ├── AppSidebar.vue        # 侧边导航 + 计数
│   │   ├── AppTopbar.vue         # 名称/时钟/搜索/新建
│   │   ├── TaskList.vue          # 列表容器 + 筛选排序
│   │   ├── TaskItem.vue          # 事项行（勾选/进展/菜单）
│   │   ├── TaskQuickAdd.vue      # 快速添加
│   │   ├── TaskDetail.vue        # 右侧详情面板
│   │   ├── TaskEditor.vue        # 新增/编辑弹窗
│   │   ├── ProgressTimeline.vue  # 进展时间线
│   │   ├── ConfirmDialog.vue     # 二次确认
│   │   ├── EmptyState.vue        # 空状态
│   │   └── ToastHost.vue         # 轻提示
│   ├── views/                    # 路由页面
│   ├── stores/                   # Pinia：tasks / ui / confirm / toast
│   ├── services/                 # api（唯一 IPC 出口）+ format
│   ├── types/                    # 与 Rust 模型对应的 TS 类型
│   ├── router/
│   └── styles/main.css           # 设计变量 + 基础样式
├── src-tauri/
│   ├── src/
│   │   ├── commands/             # 命令层（无 SQL）
│   │   ├── db/                   # 仓储层（可单测）
│   │   ├── error.rs              # 统一错误 + 序列化
│   │   ├── models.rs             # 领域模型
│   │   ├── state.rs              # 全局状态
│   │   ├── lib.rs / main.rs
│   ├── migrations/001_init.sql   # 数据库结构
│   ├── capabilities/default.json # Tauri 权限
│   └── tauri.conf.json
├── scripts/
│   ├── env.ps1                   # Rust/MSVC 环境
│   └── make_icon.py              # 生成应用图标
├── docs/
│   ├── ARCHITECTURE.md
│   ├── DATABASE.md
│   └── TESTING.md
└── README.md
```

---

## 七、数据存放与备份

- 数据库：`%APPDATA%\com.shiftnote.desktop\shiftnote.db`
- 备份：`%APPDATA%\com.shiftnote.desktop\backups\shiftnote-backup-<时间>.db`
- **恢复**：设置页 →「从备份恢复」→ 选择 `.db` 文件 → 确认。系统会先把当前数据
  快照为 `backups\pre-restore-<时间>.db`，再覆盖；失败会自动回滚。
- 程序启动时若检测到数据库损坏，会把原文件另存为
  `shiftnote.corrupt-<时间戳>.db` 后重建空库，并在界面上提示。

详见 `docs/DATABASE.md`。

---

## 八、已知限制（将在后续阶段补齐）

1. 提醒调度器尚未接入（阶段 4）：`reminders` 表与“完成/隐藏即停提醒”的逻辑已实现，
   但还不会主动弹通知。
2. 交接班模块未实现（阶段 5）。
3. 系统托盘、迷你浮窗、开机自启、全局快捷键未实现（阶段 6）。
4. 列表拖拽排序的命令已具备（`task_reorder`），界面拖拽在阶段 2 收尾时接入。
5. 批量完成 / 批量隐藏、自定义时间范围筛选、已归档视图待补。
