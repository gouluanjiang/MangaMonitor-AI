# Matcher M2 — 确定性身份实现与受控离线接入

基线：`main@d0c321cef4df01df85526181ecc6438ca8abc371`，接手时工作区干净。
M1 已验收，本阶段未重做 181 条分组。规则版本：`matcher-m2-v1`。

## 实现范围

`rules-core::title_m2` 是新语法解析器；M1 `title_identity` 及其 30 项测试保留。
新解析器不以“出现数字/上/下/第”授权身份字段。普通正文全部保留；只解析完整后缀语法、
完整边界 annotation 和封闭词条。每条证据记录 rule、field、text、reference、span；
span 为 **normalized 字符串的 UTF-8 byte offsets**，raw 全文另外保存，绝不把规范化偏移
冒充原始字符偏移。外部 content_type/fandom 字段带独立来源记录。

`Fields` 包含 core 外的 series_number、volume、episode、chapter、part、front_back、
upper_lower、extra、collection、content_type、subtitle、fandom。数字字段为
`Numbering { first, last }`，last 有值表示该字段单位的范围；None 表示未明示，**不是 false**。
没有把空 collection/extra 推为“确认无附加内容”。原始编号字面值保留前导零；范围分隔符
只在已经识别的数值语法内规范化。不转罗马数字、不全局简繁转换、不删标点。

`cloud-monitor::matcher_m2::decide/replay` 使用生产 State，并通过抽出的 `State::bind_work`
复用原版本比较、UNKNOWN、pending、coverage 安全逻辑。`matcher-m2` binary 是唯一新增 CLI
入口，只接受现有 observations/export/seed/repair/resume/output 路径；拒绝 live-source 等选项。
原 `phase3a/phase3b` 的默认行为保持不变，本次**只接入受控离线 identity/downstream state replay**。
这不是分页/可用性 rescan，也没有把 M2 自动启用到定时生产扫描。

## 批准的规则、边界与碰撞测试

| 类别 | 明确边界与 provenance | 碰撞保护 |
|---|---|---|
| NFKC/casefold/空白 | 原有 conservative_title；raw 保留 | 前导零、标点、结构字段不丢失 |
| 语言/发行/修正 annotation | 当前首尾完整方括号内的封闭 FLAGS；例 jm:302458 | 内部 `[DL版]`、混合“翻译+第3话”、未知括号不移除 |
| Cxx / COMIC1☆n | 仅当前最前方、完整圆括号；C 后 2–3 位数字或 COMIC1☆ 后 1–2 位数字；jm:149633 等 | 裸 token、后置 Cxx、COMIC 99、C97 extra 不移除 |
| 作者 | 当前最前方完整方括号，整 token 与已确认作者规范化相等 | author UNKNOWN 不授予规则，子串不匹配 |
| 社团/作者 | 6 组封闭 pair，社团和括号内作者分别 exact；空白位置可规范化 | 未知社团、其他作者、10駅 不冒充 10驛 |
| 汉化组 | 仅当前最前方、完整方括号的 6 个 exact 词条 | 不使用“包含汉化”规则；同名带额外前后缀拒绝 |
| fandom | 仅最后方完整圆括号，12 个封闭词条或正式本地独立字段 | 保存为身份字段；缺失/不同 fandom 不等同；不批准 Fate 标点别名 |
| 编号 | 后缀 ASCII 数字 1–999，保留字面；空白边界或 3 个真实 series stem 的粘连数字 | 年份、95式、年级学期、数字正文保留，不猜编号单位 |
| volume/episode/chapter/Part | 完整后缀 `第n卷/巻/话/話` 或英文 `volume/vol./episode/chapter/part n`；可位于完整后置括号 | 单字命中不建字段，重复字段拒绝；chapter 与 episode 单位保持不同 |
| 前后/上下 | 独立后缀词或完整后置括号 | `上海`、`上上下下的日常` 不被当分篇；前≠后，上≠下 |
| Extra/bonus/after-story | 独立完整后缀或括号；extra/bonus 不互相猜等价 | 正文残余有风险标记时 veto，不能靠粗词法授予字段 |
| collection | 独立完整总集篇/総集編/collection 后缀 | 不推导成员/完整性/coverage；单话≠范围≠总集 |
| content type | 明示外部 content_type、whole tag/category 类型 token 或完整边界 `[manga/CG/…]` | UNKNOWN 不默认 manga；不认“短篇/同人/页数”作漫画类型证明；冲突拒绝 |
| subtitle | 末尾 ` ―副题―` 的闭合语法 | 副题存字段，不删除后比较；其他副标题正文保留，未解析括号 review |

社团 pair 为：3104丁目/3104、ろいやるびっち/haruhisky、HoneyRoad/Bee導師、
Hello Girls!/10驛、一億萬軒茶屋/2-G、ちまた/2-G。真实 provenance 可在原 Phase 3B
review-export 中核对；对应 parser evidence 标为 `PHASE3B_CLOSED_CREATOR_PAIRS`。
汉化组为：空氣系☆漢化、路人漢化、天魔的黑兔個人漢化、不可視漢化、不夠色漢化組、灰羽社漢化；
代码每个词条直接保存 source key reference。
允许粘连编号的 series stem 为：くーねるすまた、はるこす、配達バニーガールとサービスえっち。
所有类别都有 grammar/negative/collision tests，集中在 `rules-core/tests/matcher_m2.rs`。
没有批准 publication 杂志年月号去除，也没有批准通用双语拆分或标题字形 alias。
前篇/前編、后篇/後篇/後編等仅在已解析的分篇字段内编码为相同枚举意义，不传播到 core。

## 四类决策与证据链

1. `AUTHORITATIVE_EXISTING`：人工 SAME 或同站 source mapping；矛盾/多映射先 review；IGNORE 保留。
2. `AUTO_EXISTING`：作者已确认，完整 title fields + core + 明示内容类型一致，唯一 work；
   所有同作者 scope 候选均审计，未解析本地候选/相同 core 的未知字段不能隐藏碰撞。
3. `PROVEN_NEW`：仅独立确认并绑定 inventory hash 的完整作者 scope certificate，且所有现有
   work 都在同一个完整系列身份中由一个明确、非范围的编号字段证明不相交；前导零拼写差异、
   extra/collection/范围、空 scope、未索引 catalog 身份不能据此判新。离线 CLI **不接收该证书**，
   本批五作者种子不完整，因此不会把无匹配/同作者异名记录变成新作。
4. 其他情况 `REVIEW_REQUIRED`：具体 reason + 全部解析问题，不再使用泛化 UNRESOLVED_TITLE_IDENTITY。

`scope_work_ids` 只是检索范围；`matching_work_ids` 只记录标题对应证据，未知类型时它不授权
绑定。`candidate_evidence` 保存每个 work 的本地 identity、content evidence、relation、NOT_SAME
排除信息。每个 Outcome 包含 source key、work_id、author evidence、source identity、移除 metadata、
结构字段、类型证据、唯一性说明、下游结果。不能把 identity match 与版本升级许可混为一谈。
SAME 不等于删除许可，coverage 未放宽。无 AI runtime、fuzzy threshold 或语义翻译。

## primary=null 的独立数据修复

正式 `inventory.json`（2833 items）中的 `LOCAL_ITEM_2705` / `WORK_02657` 文件名完整，
作者已经独立确认为 2-G；旧 title parser 仅因 `NO_SAFE_CREATOR_ANCHOR` 未提取标题。
`fixtures/matcher-m2/inventory-primary-repair.json` 保存原 item、原文件 SHA-256、line_number、
work/local IDs、expected before 和 expected after。按 exact `.zip` 后缀和封闭 `(オリジナル)`
fandom 语法恢复 primary；不猜作者，不从 source title 反向填本地标题。

修复仅应用于 M2 staging 副本，幂等，拒绝覆盖任何不同的已有 primary/作者/local ID。
恢复的标题为 `竿役募集してる推しの爆乳エロ配信者が妹になりました`。原 17 条记录共用的
空 primary 问题已在离线路径解决，但这不等于 17 条都能绑定；未知类型等阻断仍有效。
正式 seed 继续保持原状，未导入 2833 全库存或完整作者名单；M1 审计输入 hash 不改变。

## 离线证据与复现

`fixtures/matcher-m2/phase3b-observations.json` 从本地已有
`reports/phase3b-run-33998989019/phase3b-first/observations.json` 原样复制。
对应成功 run `33998989019`、artifact `9979046120`；本轮只查询了 GitHub artifact 元数据，
没有重新下载 observations，也没有源站请求。原始 tape SHA-256：
`ce9d43ee711cc456ed1069cac78da9267d85122d858a7b117e2395ae60f83224`。
CLI 对 215 条 source key、raw title、authors、metadata 与已提交 review-export 逐项核对。

示例（输出目录必须不存在，父目录已存在）：

```text
cargo test --workspace --locked --offline
cargo run -p cloud-monitor --bin matcher-m2 --locked --offline -- --state monitor-state --export evidence/phase3b-run-33998989019/review-export.json --observations fixtures/matcher-m2/phase3b-observations.json --repair fixtures/matcher-m2/inventory-primary-repair.json --output reports/m2-first
cargo run -p cloud-monitor --bin matcher-m2 --locked --offline -- --state monitor-state --export evidence/phase3b-run-33998989019/review-export.json --observations fixtures/matcher-m2/phase3b-observations.json --repair fixtures/matcher-m2/inventory-primary-repair.json --resume reports/m2-first --output reports/m2-second
```

M2 新增 53 个 Rust tests：31 个 parser、20 个生产身份/状态规则、2 个 CLI。
其中包含全部 24 条 M1 adversarial cases 在新 matcher 上重跑、11 条 wrapper 碰撞反例、
9 条普通正文边界样本，以及 type/authority/唯一性/新作证明/缓存失效/幂等/输入保护测试。
Windows 当前完整 workspace **170/170 PASS**，原有 117 项均保留。
Linux 验证由 `.github/workflows/matcher-m2.yml` 手动运行：完整 workspace tests，随后设置无效
网络代理执行两次 captured-detail replay，并比较 checkpoint 字节及检查 seed 无改动。
最终运行 ID、统计、验证结果见 `MATCHER_M2_RESULTS.md`（验证完成后补齐）。

## 本阶段边界

production_enabled=false；五作者种子不变；无全作者扫描，无 JM/Pica 新请求，无漫画/图片下载，
无 updater exe、自动替换或删除。不进入 M3。未批准的 wrapper/别名/字幕格式继续 review，
不为降低 review 数量补猜测类型或翻译关系。完成报告后停止等待验收。
