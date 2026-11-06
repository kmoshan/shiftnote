# 上游版本检测兼容检查

上游基线：`2d67e1f`，包含 `05a503b`（检测更新）和补回 opener JS 依赖的修复。本地功能基线为 `943a9ca`，版本 0.2.1。

## 影响及处理

| 范围 | 检查结果与适配 |
| --- | --- |
| 启动口令 | 自动检查移到工作区加载后；首次未选择或加密库未解锁时，不读取更新缓存、不启动自动联网。跳过口令后的普通模式可正常检查。 |
| 备份与业务数据 | 更新命令仅读取公开版本，不调用恢复、迁移或写入事项。检查时间通过现有 settings 命令保存，遵循当前明文/加密存储模式与失败回滚。 |
| 版本 | 保留本地 0.2.1；上游发布 0.2.0 时返回 hasUpdate=false，只有更高的有效 semver 才提示。无效版本明确报错，不伪装为最新。 |
| 页面及命令 | 保留事项、交接、加密命令，合入 update 命令、opener 插件、顶部通知及设置页按钮；保留口令可选入口和原生启动诊断。 |
| 依赖 | 合并 JS/Rust opener、reqwest、semver、Windows winreg；保持 SQLite 3.54.1、加密库、Node engines、修复后的传递依赖及锁文件。 |
| CSP 与打开网页 | 网络请求在 Rust 中执行，正式 WebView CSP 不放开外网。opener 权限仅允许项目 Releases 根页面和 tag 页面，使用默认浏览器；前后端同时校验地址。 |
| 网络边界 | 固定 GitHub API HTTPS，最多 3 次同主机重定向，单次请求总超时 10 秒、连接 6 秒；响应最多 1 MiB，说明最多 16 行/4096 字符。直连失败可按上游逻辑读取系统代理重试一次，不更改代理配置。 |
| 隐私 | 会发送应用版本 User-Agent、IP 和普通请求元数据；不发送事项、口令、数据库或备份。更新检查时间保存在本地，已修正“完全不联网”类描述。 |
| 安装与签名 | 目前只检测、提示、由用户点击打开下载页；没有自动下载、替换 EXE 或静默安装。不会解决未签名的 SmartScreen 提示。 |

12 小时检查缓存会忽略未来时间戳，避免恢复备份或系统时间回调后长时间停止检查。已加密的数据库仍需原口令，不因更新功能降级。旧版程序不能读取新加密格式或保证兼容 v3 结构，更新界面不提供自动降级。

## 验证范围

- 完整 Rust 测试 112 项；另有默认忽略的真实联网测试，手工执行后成功读取公开 Release，结果为 `current=0.2.1, latest=0.2.0, has_update=false, used_proxy=false`。
- Vue 测试 71 项，覆盖解锁前无更新检查、解锁后触发、12 小时缓存、未来时间戳、重复请求、失败重试、较新本地版本和链接范围。
- 浏览器测试 7 项通过，覆盖原事项/交接/口令流程，并新增解锁→更新提示→打开页面 IPC→设置页手动检查。浏览器使用模拟 IPC，不能替代原生浏览器启动验收。
- 全量 npm 审计 0 告警；Windows Release/NSIS 重新构建成功。执行方式与产物位置见 [测试记录](TESTING.md)。

## 新增依赖公告

对相比本地功能基线新增的 83 个 Cargo 包逐个查询 OSV，命中 3 条 **INFO / Unmaintained**：[`zvariant` 5.15.1（RUSTSEC-2026-0370）](https://rustsec.org/advisories/RUSTSEC-2026-0370.html)、[`zbus_names` 4.3.5（0371）](https://rustsec.org/advisories/RUSTSEC-2026-0371.html)、[`zvariant_derive` 5.15.1（0372）](https://rustsec.org/advisories/RUSTSEC-2026-0372.html)。公告指这几个独立 crate 已合入 zbus 6，不提供旧 crate 的修复版本；这些命中不是 3 个已证实可利用的漏洞。

`cargo tree --locked --target x86_64-pc-windows-msvc -i zvariant` 和 `-i zbus_names` 均无依赖；Linux 目标则存在 `tauri-plugin-opener 2.7.2 → zbus 5.19.0 → 这些组件`。本次不通过强行替换传递依赖跨越 zbus 主版本，后续需跟随 opener 的 Linux 依赖迁移。既有 Linux GTK/glib 风险及其他停止维护提示仍按安全审查记录保留，不能据此宣称整个 Cargo 锁文件所有平台零告警。

原生完整交互、真实安装升级和另一台物理机器仍不在已完成验收范围。真实 GitHub 请求成功不代表已手工验证默认浏览器能打开；仅证明 Rust HTTP/版本解析链路可用。
