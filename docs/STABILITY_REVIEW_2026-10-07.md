# 全项目稳定性审查与修复（2026-10-07）

## 基线与结论

本次审查基于 `main@012146b1a78c76013019276fe29b7f6d42099ab7`，覆盖
工作区 11 个 Rust crate、独立 Tauri 桌面 crate、React 界面、下载与磁盘恢复、
账号与来源网络、阅读器生命周期、依赖锁文件及现有验证链路。
核对完整来源文件和相关既有回归，并对下载清理、JM 预检和 UI 生命周期交叉审查。
提交前 main 已独立合并仅涉及 CI 的 PR #31；本修复以最新
`f90bdae7a7c9d6cee9a4553775780346d796f860` 为父提交，应用审查基线未变。

项目已有严格的失败拒绝、文件身份、会话代次、原子文档与资源预算保护。
本轮发现并修复了几个会增加人工维护量的具体缺口：暂停后的 JM 预检继续占用队列、
完成下载的暂存清理证明可能随历史整理丢失、浏览位置和旧会话缓存保留过久，
以及页面原点可能在后续布局测量时漂移。另更新三项命中公开公告的依赖补丁。

这支持一次有明确目标的稳定性维护，不支持“永久无需维护”或“所有运行环境均已验收”的保证。
正式验收以本修复 PR 当前 head 的 CI 状态和证据链接为准；源码审查、独立内存诊断、
合成 CI、用户电脑上的交互验收和生产接受是不同层次的证据。

## 与 PR #30、#31 的关系

[PR #30](https://github.com/gouluanjiang/MangaMonitor-AI/pull/30) 已合并到上述基线。
其 fail-closed 安装后校验、隐私诊断和真实 NSIS lifecycle 已独立审查，测试修正提交只
修正 PowerShell 调用边界，不移除 32 项安装校验断言。本轮保持这些门禁。

| 原 PR #30 同一来源 head 的关键证据 | 运行 |
| --- | --- |
| Windows desktop / 真实 NSIS 生命周期 | [37283555016](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/37283555016) |
| Rust 与跨平台 baseline | [37283555011](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/37283555011) |
| UI | [37283554989](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/37283554989) |
| Windows install verification contract | [37283555001](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/37283555001) |

合并后另一次 main push 的 baseline
[37613134117](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/37613134117)
和 Windows contract
[37613134014](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/37613134014)
通过；UI
[37613133963](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/37613133963)
出现已记录的冷启动 `scrollTop=15`、期望 `0`，其余 268 项浏览器用例通过。
desktop
[37613134105](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/37613134105)
被取消，不能列为本轮成功证据。

[PR #31](https://github.com/gouluanjiang/MangaMonitor-AI/pull/31) 的 Node Action 与
Ubuntu 迁移已独立合并。其新 main 的四组 push CI 均成功：
[baseline 37613565802](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/37613565802)、
[UI 37613565765](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/37613565765)、
[desktop 37613565797](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/37613565797)、
[contract 37613565936](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/37613565936)。
这个应用源码未变的成功 UI 运行不单独证明 LOCAL-03 已修复。
本批相对该新 main 没有修改 workflow、Action pin 或 runner 版本。

## 已确认问题与修复

### 1. JM 下载预检在暂停后仍可能继续整本书的请求（P2）

原 `run_for_download` 的 JM 分支只在完整章节枚举前后检查授权。
用户在一个请求进行时暂停、注销或开始安全退出，后续章节及固定域名 failover
仍可能继续，旧 worker 长时间占用单 FIFO。末尾授权仍阻止失效结果进入媒体下载和登记，
因此这是取消响应与队列占用问题，没有据此发现错误入库。

修复复用 `request_with_guard`，在目录、每章和每次物理域请求前重新检查当前任务和会话。
完整章节循环移入 `JmClient::preflight_with_guard`，任一错误均不返回成功前缀。
JM/Pica 桌面入口共享逐请求检查和末尾授权；既有 `run_live` 前后授权语义保留。
固定域名、headers、协议解密、重试分类、物理 trace 和完整图片计数保持。

位置：`crates/jm-adapter/src/lib.rs`、`crates/cloud-monitor/src/live_source_preflight.rs`。
新增 7 项合成 adapter 回归；cloud 首请求撤权回归直接覆盖两源，既有 JM 末尾变化用例
覆盖共用的 finalizer。没有真实来源请求或媒体下载。

### 2. 已完成下载可能失去清理残留所需的唯一证明（P2）

原流程先持久化 `Downloaded`，随后尽力清理暂存。清理失败或两步之间进程退出时，
重开后的完成任务没有清理入口；“移除历史”只保留 compact identity，丢弃完整暂存清单、
哈希和执行证明，残留临时媒体以后只能人工处理。

修复允许空闲的已完成任务明确重试现有 `cleanup` 操作。最终输出、完整 manifest、
暂存允许清单、逐文件哈希及 workspace 独占锁仍须通过；成功后保留原完成状态、revision、
时间戳和 library entry。历史整理在持锁期间确认所有选中任务各自的 command 目录确切不存在，
有残留、重定向或无法确认就保留全批记录，不丢恢复证明。

清理先核验全部剩余内容，再执行精确删除；补齐删除最后的 `chapters` 后只留下空 command
根的中断恢复。缺失整个 staging 或 `commands` 的旧状态不会永久阻塞历史整理。
没有新增重开自动删除、后台自动删除、任意路径删除或最终漫画删除。

位置：`crates/workbench-downloads/src/service.rs`、`materialize.rs`，以及下载 UI/runtime。
新增 8 项 Rust、2 项 UI 逻辑和 1 项浏览器生命周期回归，覆盖重启、未知/改变文件、
锁和旧 revision、批量保留、符号链接、清理尾部中断、旧目录缺失。

删除阶段不是文件系统事务：后续 I/O 失败仍可能留下已删除的合法前缀。
保证的是完整记录和证明继续保留，允许明确重试；不声称所有失败时每个暂存字节都不变。

### 3. 查询组合和旧账号会话导致浏览缓存长期保留（P2）

原全局浏览位置 Map 为每个 query/filter/sort scope 保留完整作品 key 数组，没有容量或寿命上限。
关闭作者标签的已有清理没有覆盖普通页面的历史查询组合。最近更新 reader/view、来源 savedViews
和作者 membership 也会保留已经失效的账号 session；旧响应隔离已存在，本轮没有发现跨账号串显。

修复为全局位置和状态增加 LRU，同时限制数量、保留 key 数和逻辑字节成本。
每个挂载列表只额外保留一个有界的当前位置，保护当前及隐藏但未关闭标签；旧查询变体可淘汰。
关闭作者标签以 owner 清理，失效 session 的缓存、fallback 和晚到 cleanup 写入均被回收或拒绝。
来源切换仍保留当前有效账号的列表和位置。

| 缓存预算 | 上限 |
| --- | ---: |
| 全局浏览位置 | 128 份 |
| 全局保留作品 key | 65,536 个 |
| 全局位置逻辑字节成本 | 8 MiB |
| 单位置邻近 key / 逻辑字节成本 | 4,096 个 / 512 KiB |
| 全局 query/filter 状态 | 128 份 / 256 KiB |

逻辑字节成本按 UTF-16 文本、引用和条目成本估算，不是 V8 heap 上限。
用户持续增加打开标签时，各页面当前内容仍占内存，不能将全局缓存预算等同于全应用恒定内存。

独立 Node 24.19.0 合成诊断使用 160 个 scope、每份 2,000 个 key，跨事件循环后执行 GC：

| 同参数诊断 | 基线模块 | 修复模块 |
| --- | ---: | ---: |
| GC 后仍保留位置 | 160 | 32 |
| 仍保留 key | 320,000 | 64,000 |
| heap 相对增量 | 56.25 MiB | 11.32 MiB |
| 明确 forget 后相对增量 | 0.42 MiB | 0.44 MiB |

修复模块输出的全局逻辑成本为 4,748,736 字节；此例首先受到总 key 预算约束。
这说明特定强引用保留已被限制，不能归因为 Windows 全应用所有内存增长的根因。
复现脚本：`apps/local-workbench/tests/soak/browse-retention.mjs`，命令
`node --expose-gc tests/soak/browse-retention.mjs`。这次没有重跑本机完整 UI/原生/四小时 soak。

新增 5 项位置/状态预算回归、1 项 membership 会话回收回归，以及隐藏作者标签在超过
全局容量的查询压力后仍保持当前位置的浏览器回归。

### 4. 页面顶部被后续网格测量移动（P2）

最新失败 trace 表明冷重载后页面曾在顶部，后续一帧变为 15 像素。
原网格锚点使用旧 grid offset，布局更新后用新 offset 重建 scroll；当页面原点的上方布局
增加 15 像素且行高重测时，这种卡片锚点会将原点也移动 15 像素。

`VirtualSourceGrid` 现在在实际 `scrollTop === 0` 时捕获明确的 `atTop` 语义，
后续恢复保持页面原点；中间位置继续使用原卡片锚点计算。位置缓存和邻居回退保留此标记。
新增确定性的 15 像素标题变化与行高变化回归，并同时检查中段位置仍保留。
原冷重载回归仍严格要求 `0`，没有放宽阈值、跳过或添加自动重试。

该修复需要本批 CI 核验；历史 LOCAL-03 不能仅因未改代码的成功重跑而被宣布修复。

### 5. 公开依赖公告的最小补丁更新

| 依赖 | 锁定值更新 | 公告与适用说明 |
| --- | --- | --- |
| Tauri Rust runtime | 2.11.5 → 2.11.6 | [GHSA-w28w-mhc8-qvjv](https://github.com/tauri-apps/tauri/security/advisories/GHSA-w28w-mhc8-qvjv)：大 IPC 响应的跨 WebView 取回缺少归属约束。项目有权限不同的本地 WebView，值得优先更新；没有发现从外部内容进入脚本执行的完整利用链。 |
| rustls（两份 Cargo.lock） | 0.23.43 → 0.23.45 | [RUSTSEC-2026-0285](https://rustsec.org/advisories/RUSTSEC-2026-0285.html)：TLS 1.3 对错误加密层级的握手消息接受过宽。握手 transcript 认证仍存在，不据此声称网络攻击者可伪造完整握手。锁文件匹配不等于证明每个平台都启用了该 backend。 |
| source-map-js（构建依赖） | 1.2.1 → 1.2.2 | [GHSA-68fv-2mgg-jv7q](https://github.com/advisories/GHSA-68fv-2mgg-jv7q)：恶意 indexed source map 的偏移可阻塞事件循环；此仓库路径为 Vite → PostCSS 的构建依赖。 |

Tauri 两个精确 manifest pin 与 registry lock checksum 一起更新；crates.io index 的
2.11.5/2.11.6 依赖和 feature 元数据相同。rustls 提高的 webpki 最低版本已由现有
0.103.15 满足，未启用的 aws-lc 不引入锁文件。registry checksum 均从官方 index 核对，
最终 Cargo 一致性仍由 `--locked` CI 确认。没有更改 Tauri CLI、JS API 或全量更新传递依赖。

对根锁文件 221 个 registry package、原生锁文件 512 个 registry package，使用固定的
RustSec advisory-db revision `f246cde705ecb3a6b421d6d5462c6d88f317db5f` 做包名与版本范围核对。
115 个包名相关记录中 102 个未撤销；补丁前另匹配 rustls，以及 glib 的 unsound 公告和
6 项 unmaintained 信息。glib 0.18.5 属于 GTK/Linux 原生依赖链，不能直接替换成不兼容的
0.20 并当作本批 Windows 补丁；应随上游 backend 维护。未维护信息涉及 proc-macro-error
和 5 个 unic 包，没有据此宣称存在同等严重的可利用漏洞。

这是一份明确范围的静态公告核查，不冒充运行过 `cargo audit` 或完成全依赖可达性证明。
另行检查官方 Tauri 公告补足公告库覆盖差异。npm advisory audit 在补丁前报告 1 个 high
构建依赖问题，补丁后同一锁定图报告 0 个已知告警；该结果不保证未来没有新公告。

## 长期运行仍有的边界

| 边界 | 实际含义 |
| --- | --- |
| 来源网站与账号 | 超时、限流、拒绝访问、登录失效或协议变化可以安全停止，仍可能需要重新登录或适配器维护。 |
| 持久 observation / recent 容量 | 全局 observed 上限 100,000 条 / 64 MiB；每账号 recent IDs 20,000 条。没有自动丢弃原始记录或未读内容的保留策略。 |
| 下载历史 | 完整任务 500 条 / 32 MiB，abandoned 另有 500 条边界；compact identity 20,000 条。已有明确历史和暂存整理入口，不能无限保存。 |
| RECENT_LIMIT 提示 | 已改为准确说明：刷新可重读当前页面，不会释放已保存的持久历史容量。没有私自淘汰历史。 |
| 输出创建到身份持久化窗口 | 极小中断窗口仍可能留下不能证明所有权的目标文件/目录；保持 `DOWNLOAD_DESTINATION_EXISTS`，不能自动接管或删除未知目标。 |
| 安装验收 | 2026-10-03 本机 NSIS 临时文件失败根因仍未确定；不声称历史版本升级或用户机器交互/native UI 已通过；uninstaller 仍无发行哈希校验。 |
| 已交付版本 | 当前公开 1.0.2 与用户安装不会因合并维护代码自动更新。本批不发布、不替换 release asset、不在用户电脑安装。 |
| 历史本机矩阵 | 沿用 28 PASS / 8 WAIVED / 1 UNTESTED 的实际层次；不恢复用户已豁免事项，B06 保留自然更新体验边界。 |

下载、登记、历史整理、暂存清理与最终漫画回收的权限继续分开。
未知身份、损坏文档、网络失败、分页不完整或内容变化，不解释为“作品不存在”或“可以删除”。
云监控保持 `production_enabled=false`；本轮没有真实账号、来源或漫画库变更。

## 后续维护原则

正常使用不需要定期重启来清空本次修复的全局浏览位置缓存，也不应手工删除无法确认身份的文件。
只有应用明确提示暂存或历史容量需处理时，使用对应的已验证入口。
安全公告、来源协议变化和依赖支持终止仍需要维护；按具体风险提交最小补丁、复用现有 CI，
避免无理由全量升级和重复跑长时间验收。runner/Action 生命周期维护继续与应用修复分开。

进入下一次正式发行前，应以最终提交重新核验受影响 CI 与实际安装链路，并按既有范围处理
必要的用户机器验收和数据备份；本审查不新增未获授权的生产或发布操作。
