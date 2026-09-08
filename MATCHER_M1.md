# Matcher M1 — 冻结设计与 evidence 分析

状态：M1 完成，等待 ChatGPT 验收；不授权 M2，不接入生产 auto SAME。
起点：`86b8e13ea856cc8e707c071841ee8da8486c1fb3`，main，开始时工作区干净。

## 数据边界与可复现性

只读取已提交的 `evidence/phase3b-run-33998989019/review-export.json`、原因/作者摘要，
以及关联候选所必需的现有五作者 `monitor-state/inventory_index.json`。
后者用于解释本地 primary、fandom、content 数据质量，不是新来源数据。
仓库没有已跟踪的 Phase 3B `observations.json`，没有下载 Actions artifact 补齐它。
没有请求 JM/Pica、扫描、下载图片、改动库存、作者名单、decisions 或生产开关。

`scripts/Analyze-MatcherM1.ps1` 生成 `fixtures/matcher-m1/evidence-analysis.json`。
运行 `./scripts/Analyze-MatcherM1.ps1 -Check` 可核对完整输出和两个输入 SHA-256。
每行保存 source_key、review_id、原始标题、全部候选 primary/fandom、字面特征命中、
诊断分组和人工审阅的差异线索。分析脚本使用 .NET NFKC/lowercase 做字面检索，
不作为 Rust runtime 的 Unicode casefold 实现或身份授权。

## 181 条的互斥诊断分组

按下表从上到下的优先级归类；不是“可自动解决数量”，不是已确认的真实作品关系。

| 诊断组 | 总数 | JM | Pica |
|---|---:|---:|---:|
| NULL_LOCAL_PRIMARY：所有候选 primary 缺失 | 17 | 9 | 8 |
| MALFORMED_BRACKETS：括号不平衡/错配 | 5 | 4 | 1 |
| IDENTITY_SENSITIVE_OR_LOW_INFORMATION：范围、extra、合集、类型或 artist-only | 9 | 4 | 5 |
| LOCAL_PRIMARY_LITERAL_WITH_RESIDUAL：含完整本地 primary 字面片段，仍有残余 | 21 | 17 | 4 |
| LOCAL_REPRESENTATION_DIFFERENCE：逐条审阅的字形、双语或本地 wrapper 差异 | 10 | 6 | 4 |
| NO_LITERAL_LOCAL_PRIMARY_WITNESS：没有上述本地主标题字面证据 | 119 | 73 | 46 |
| 合计 | 181 | 113 | 68 |

119 条只能断言“当前证据未提供本地 primary 的字面对应”，不能断言是新作或 NOT_SAME，
也不能猜测翻译关系。全部 181 条都有同作者候选；这正是生产 `possible` 扩散的机制。
`monitor.rs` 会在标题/类型验证前将同作者本地 work 插入 possible；未达到 exact 时即 review。
`2-G` 的 17 条都只有 `WORK_02657`，其 primary/normalized_key 为 null，任何标题清洗都不能修复它。

21 条字面包含不等于 wrapper-only 正例。例如 `jm:1468461` 包含本地 `アオハルトラレ`，
但还带 `爆乳女教師・尾奈ほなみ編`；不得删除副标题后合并。
`jm:371595` 重复了标题，`jm:616490`、`jm:626260` 含无分隔的翻译前缀，必须保留。
`jm:1016609` 在 119 条中：author evidence 为 `10驛`，标题 wrapper 却为
`Tear Drop (ついな)`；不能把任意“社团(作者)”当已确认作者 metadata。

## 可重叠的词法特征统计

这些计数基于完整原始标题，非互斥、非身份判断；正则及实际命中文字均保存于分析文件。

| 特征 | 数量 |
|---|---:|
| Cxx / COMIC1 活动 wrapper | 63 |
| コミックホットミルク 年月号 publication wrapper | 4 |
| 社团(作者)形式 wrapper（仅语法形状） | 99 |
| 汉化/翻译/重嵌/润色 annotation | 86 |
| 封闭语言 annotation | 67 |
| DL/digital/无修正等版本 annotation | 82 |
| 显式 `|` / `丨` 双语分隔符 | 3 |
| NFKC 改变原文 | 8 |
| 大小写改变原文（含 metadata） | 139 |
| 原文数字（含活动号/作者数字，不能当卷号） | 117 |
| range | 2 |
| Extra/bonus/after-story/おまけ/外传词法标记 | 5 |
| collection/总集编/オムニバス词法标记 | 3 |
| CG / artbook 明示词法标记 | 1 / 1 |
| 明示卷、话/chapter/episode、Part、前后篇、上下篇 | 各 0 |
| novel / settings 明示词法标记 | 0 / 0 |

零命中表示本批缺乏该语法证据，仍必须用 adversarial fixture 覆盖。
NFKC 能处理全角数字/字母/标点，不能做简繁转换；`jm:1127144`、`jm:1127145` 的组合
浊点也属于真实 NFKC 变化。脚本用 Ordinal 比较，避免 PowerShell 文化比较忽略规范等价编码差异。
双语无显式分隔的前缀不计入 3 条；本地 `WORK_01173` 自身含 `丨` 双语 primary。
`collection` 命中只是风险提示，不能把 `Haruhi Lingerie Collection 01` 猜成漫画总集篇。

## 冻结的身份模型

生产目标结构应保存：raw title、schema/rule version、规范化后的完整身份串、core title、
series/number、volume、episode/chapter、part、前后/上下篇、range（及单位）、extra/bonus/
after-story、collection、content type、fandom/subtitle、双语候选、未解释残余、解析问题、
每个移除或提取步骤的规则/原文/span/provenance。未知字段保持 UNKNOWN，不是 false/空集合。

M1 的纯函数原型可以先保留完整 core text 并提取结构证据；没有确定语法时不得强行把
字符串截成“正确主标题”。结构 token 必须继续存在于完整身份串中，不能只留下清洗结果。
数字 2/3、3/4；前/后；上/下；单话与范围；主篇与 extra；不同内容类型均不能被普通 normalize 抹平。
编号的单位未知时是 unclassified number，不自动等于 volume/chapter；`95式`、`２年G組１學期`
中的数字、年份/季节、`II`、`++` 都保留；不做罗马数字或汉字数字猜测转换。
Range 只提供字面范围证据，不生成 coverage；1–5 不能据此声称含第 3 话、齐全或可删除旧文件。

### 规范化与 metadata

1. 仅 Unicode NFKC + Unicode casefold + 空白折叠；不删除标点、数字、括号内容或 identity token。
   不调用兼容旧夹具的 `normalize_title` 作为身份 key。
2. 可以把**完整、平衡且位于标题边界**的封闭语言/发行标记移出 core，原文和规则要保留。
   `[Chinese]`、`[中国翻訳]`、`[DL版]` 与 `作品[Extra]` 的处理不同。
   语言、修正、彩色等属于版本证据；移出 core 不等于确认质量，冲突仍 UNKNOWN。
3. Cxx / COMIC1 只有完整前置活动 annotation 才是候选 metadata；裸 `C97`、标题内部活动串
   仍保留。**M1 原型不自动移除活动号**；是否足以授权移除必须在 M2 给出 provenance 和位置验证。
   publication 的年月号只在已确认杂志载体时可分离，不能把任意 COMIC/数字删除。
4. 作者/社团 wrapper 必须完整、唯一地符合已确认作者证据，未知社团作者与正式作者冲突则 review。
   汉化组必须 exact 的经审阅 metadata 词条和位置证明；不能用“含漢化”正则删除任意括号。
   **M1 原型不自动删除社团/作者/汉化组/publication/fandom**，避免把分析特征检测当 removal grant。
5. Fandom 和副标题为有意义的限定字段，不通用删除。`Fate/stay night`、`Fate-stay night`、
   `Fate stay night` 不靠删标点推为同一。残余 annotation 未解释则保留并阻止完整比较结论。
6. 双语分隔仅产生带出处的候选片段，不让任何片段直接覆盖原始 primary；无分隔翻译前缀不猜。
7. 不做全局简繁/日文字形归并。`编/編`、`溫/温` 是本批的人工 review 线索，M1 **不批准**
   新的标题字形等价。`里/裏`、`駅/驛` 在标题中不自动等价；已有作者专用 `10驛/10駅` 规则
   不能传播到标题。任何未来别名必须限定完整作品、字段、出处与碰撞检查。
8. 短标题、纯作者名、Artist-only、合集泛称、primary null、破损括号、解析冲突都 review。
   “同作者”只提供检索范围；长度限制是拒绝自动比较的 guard，不是 fuzzy 相似度阈值。

### 比较与生产接入边界

M1 comparison helper 最多返回 `ExactRepresentation` / `DifferentRepresentation` / `NeedsReview`。
它不返回 SAME/NOT_SAME，不写 decisions，不判新作、不授权删除；DifferentRepresentation
只是字符串/结构不一致，也不是不同作品的证明。类型缺失不能默认 manga；两个 UNKNOWN 不构成匹配证据。

未来生产 SAME 必须独立检查：人工 decisions/冲突、同站 source ID 权威映射、确认作者、唯一候选、
完整身份字段及已确认内容类型、未解释残余、版本/coverage 各自的安全逻辑。
本轮不改变 decisions SAME/NOT_SAME/IGNORE、source ID、UNKNOWN、coverage 或生产 matcher。
SAME 始终不等于删除许可，无 AI/LLM runtime，无 fuzzy/Levenshtein 授权，无语义翻译。

## 留给 M2（须另行验收和授权）

正式接入 matcher、改变 possible 候选策略、消除 review、批准 wrapper/别名表、完整结构语法及
跨字段冲突、修复本地 null primary/双语污染、完整 observations 离线 replay、生产 SAME 行为差异审查。
以上不是本阶段已经解决的问题。生产开关保持 false，作者名单保持五作者种子。

## M1 已交付实现与验证

- `crates/rules-core/src/title_identity.rs`：独立纯 Rust 原型，复用现有 NFKC/casefold，
  保存 raw/normalized/core_text、移除的封闭 annotation、带 UTF-8 span 的结构词法证据、
  Option 内容类型与问题列表。比较仅为 representation 比较，不返回 SAME/NOT_SAME。
- `core_text` **仍包含结构 token**。本轮没有完整的主标题/卷话解析器：number/range 单位未知，
  volume/chapter/part/上下等只是词法证据；CJK 单字可能命中正文，不能用这些命中授权关系。
  未实现 metadata 每项独立原始 span/provenance、双语语法树、完整 series 字段；原始全文保留。
  这些限制不会影响生产，因为 cloud-monitor 没有调用该模块。
- `fixtures/matcher-m1/real-parser-cases.json`：15 条直接取自 Phase 3B 的真实标题，
  人工指定必须保留的片段、结构标记与问题；测试核对 source_key 和未改写的原文。
- `fixtures/matcher-m1/adversarial-cases.json`：24 条反例，包含全部要求的身份差异，
  以及同作者短标题、未知类型、内部 metadata、未经批准括号、字形与副标题。
- `evidence-analysis.json`：181 条完整审计样本；测试逐条核对真实 evidence、组数、
  原文保留、span 有效和 UNKNOWN 内容类型不得成为 ExactRepresentation。
- 新增 **30 个 Rust tests**（24 个独立反例 + 6 个 corpus/parser/normalization 测试）。
  Windows `cargo test --workspace --locked --offline`：**117/117 PASS**，原有 87 项全部保留。
  `cargo fmt --all -- --check`、分析脚本 `-Check` 通过。Git diff 空白检查按既有 CRLF
  使用 `core.whitespace=blank-at-eol,blank-at-eof,space-before-tab,cr-at-eol`，避免把 CR 当行尾空格。
  续接时再次运行 M1 的 30 项最小回归全部通过，未重做 evidence 分析。
  本轮没有运行 Linux Actions；基线 Linux 87/87 是既有 Phase 3B 结果，不冒充本轮验证。
- 没有新增依赖，没有修改 Cargo.lock、生产 monitor、adapters、state、decisions、coverage、
  authors 或 workflows。没有请求新来源数据，没有下载漫画/图片，没有执行文件删除。

首个分析 checkpoint `8959ad1` 已推送；最终代码 checkpoint 由 Git 日志及交付消息给出，
避免在提交内自引用其 SHA。M1 完成后停止，等待验收。
