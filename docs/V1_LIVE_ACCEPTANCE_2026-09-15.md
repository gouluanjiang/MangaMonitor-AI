# V1 候选版代理验收报告 · 2026-09-15

Latest steering (2026-09-15): repair author attribution before further V1 work. [The shared author-evidence repair](AUTHOR_ATTRIBUTION_FIX_2026-09-15.md) separates explicit author metadata from other keyword results while preserving complete pagination and inspection access. This supersedes treating every keyword hit as an author work. Engineering and repaired native acceptance are pending; the earlier wrong-author sample only established download/receipt behavior.

**结论：工程检查通过，真实验收尚未通过。** 用户指出下载样本并非所查作者，已确认当前作者页把通用关键词命中归入查询作者。这是需要解决的作者归属问题，不能用“已读完全部分页”或范围说明替代准确性验收。新增下载和先前请求补填搜索词的动作均已暂停。

## 已修复并验证

诊断摘要原来漏计“已索引但页数为零或未知”的文件，导致待核对数量与漫画库不一致。提交 `672cf4a47daec0b892aa97f22c7086d9c25a26af` 改为共用 `fileNeedsReview`，增加健康、不可读、含错误、零页和未知页数的合成回归。修正候选版实机中，诊断与漫画库都显示待核对 18，复制提示正常。下载实现与入库规则未因这次修正改变。

用户明确授权后，提交已推送到现有开发分支。全部正式套件和构建只在 CI 执行：

| 检查 | 结果 |
| --- | --- |
| [前端 34976839669](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/34976839669) | 格式、145 条逻辑测试、类型／构建、108 条 Chromium 用例通过。 |
| [基础 34976839522](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/34976839522) | Rust 工作区、Windows 基础回归和生产开关检查通过。 |
| [Windows 34976839541](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/34976839541) | 模块／凭据检查、34 条原生测试、Clippy、EXE 构建、实际 WebView 启动与重启通过；安装包跳过。 |

## 真实观察及边界

| 项目 | 结论 |
| --- | --- |
| 会话与常用入口 | Pica 会话恢复；JM 由用户重新登录后可恢复。漫画库详情能在资源管理器选中 ZIP，设置分类快捷跳转和诊断复制正常。 |
| 手动作者检查的执行过程 | 明确点击后开始两站查询，读取中保留旧结果并标明未完成范围，切到队列再返回保留结果／筛选。此项只证明查询流程，不证明作者归属。 |
| 单本下载与 ZIP | 一例 Pica 40 页样本首次在 9 页停下，保持未入库。单次手动重试完成；ZIP 有 40 页 JPG、1 张封面与元数据，条目不重复，CRC 和下载记录中的 SHA-256 一致。 |
| 下载后的状态同步 | 无需扫描或手动刷新，Pica 当前范围从已入库 0／未入库 12 变为 1／11。样本从未入库筛选消失，已入库筛选、作品详情一致；漫画库增加一条，按入库时间位于首条。 |
| 正常关闭重开 | 新记录、文件条目、旧作者检查结果与检查时间保留，没有自动启动作者检查或重试旧失败任务。 |
| 作者作品准确性 | **未通过。** 下载样本标注的作者与查询作者不同。代理选样错误；以上下载与同步证据不能算作者更新准确性通过。 |
| 新作者输入、设置关键词 | 原生工具无法验证编辑焦点；自动审批拒绝在文档根节点焦点下输入。未绕过拒绝。此前请求用户填搜索词，目前因作者归属问题暂停。 |
| 全部入库提示、收藏／排行对新样本的同步 | 本轮没有合适的完整真实范围／榜单样本，未为测试额外下载；已有合成证据不冒充本轮实机通过。 |

首次下载只保留了 `DOWNLOAD_FAILED` 通用错误，未捕获底层原因。重试成功不能证明原因是网络或来源，也不能称该异常已修复。样本 ZIP 保留在现有漫画库，约 51.1 MiB；未删除、覆盖、整理旧库或改变网站收藏／关注。作品编号、标题、作者明细与完整路径仅放在 Git 外证据目录。

## 作者归属问题

已核查实际代码及本次保存结果：

- Pica 在 [workbench-sources](../crates/workbench-sources/src/lib.rs) 调用 `comics/advanced-search`，参数是通用 `keyword`，没有作者专属约束。
- [任意作者搜索](../apps/local-workbench/src/author-search.ts) 与 [原生作者检查](../crates/workbench-accounts/src/discovery.rs) 将所有返回项归入该查询，页面没有依据作者归属区分结果。
- 本轮 Pica 12 条记录中，6 条作者栏包含所查名字，另 6 条标注其他名字。仅凭字段差异不能证明每一条都无关，但用户指出的下载样本已经明确不属于查询作者。
- 之前百位作者抽查验证了分页读取与结果保留；它没有逐本核实作者归属，不能用来宣称作者搜索准确。

此前严格相等过滤曾漏掉合著、社团括号或不完整作者字段；本次不能直接恢复那个过滤器。应先验证来源是否支持准确的作者查询，再明确有作者依据与无法确认的结果如何呈现。保留完整分页不等于必须把无关命中计入该作者作品。当前只完成问题定位和验收纠正，尚未实施新的作者过滤方案。

## 候选版交付

- Dev 0.3.4：`Documents/Codex/MangaMonitor-Dev-20260915-672cf4a`。
- [工件 10400476223](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/34976839541/artifacts/10400476223)，PR 测试合并提交 `760898ad14334afba6b8f6867b88b0bf4a8a59ce`。
- ZIP SHA-256：`df9fb0729df96c2325c107e88ed2d184a59a7558f5c3202714f33cc982ad0beb`。
- EXE 20,352,000 字节，SHA-256：`c5f13b41e037f73bcaae3a267d87bee2c33047147581a91710354a2d80917ba0`。CRC、PE、版本和内嵌构建标识通过；已实际运行该版本。
- 没有安装包；旧 EXE 保留，草稿 PR #19 未合并，生产关闭。当前候选版包含诊断修复，**尚未修复作者结果混入问题**。

## 路线图

| 阶段 | 状态 |
| --- | --- |
| 账号／收藏、两源下载、ZIP 与同来源下载登记 | 已有实现与此前验收；本次补充单本 ZIP、同步和重启证据。 |
| 分页、作者页、榜单、完整范围选书 | 工程实现与 CI 通过；作者归属准确性仍需解决，不能将工程通过等同功能验收通过。 |
| 文件位置、设置／诊断 | 常用入口实测通过；设置关键词输入未完成，诊断计数问题已修复。 |
| **阶段 5：综合验收 ← 当前** | 先处理作者归属，再继续新作者搜索等剩余实际验收。正式 V1 尚未完成。 |
| V1 后讨论 | 旧库辅助核对、阅读器、应用内更新；当前不开工。 |

跨站身份关联、自动匹配、语言／版本猜测、不喜欢、自动替换、手机名单、分类书单等保持取消。最终证据文档留在本地交付及两个工作副本，随下次必要推送提交，不为文档重复构建。
