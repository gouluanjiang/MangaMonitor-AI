# Matcher M2 最终离线验收报告

状态：M2 实现、离线验证和跨平台测试完成，停止等待 ChatGPT 验收；未进入 M3。
起点：`d0c321cef4df01df85526181ecc6438ca8abc371`。
已推送并通过 Windows/Linux 测试的代码 checkpoint：
`c20109e342999290a49dbe1cea6b06134e813b4a`。
最终报告提交只增加 evidence/文档，不再改变已测试代码；其 SHA 见 Git 日志和交付消息。

## 215 条真实 evidence 的结果

| 结果 | 数量 |
|---|---:|
| 自动绑定 existing work | **0** |
| 人工 SAME / source mapping 权威绑定 | **0** |
| 有充分证据的独立新 work | **0** |
| 仍需 review | **215** |
| IGNORE | 0 |

review 数没有下降，这是证据边界产生的结果，不是通过调整相似度阈值压低指标。
新 matcher 已在 13 条记录中发现完整标题对应线索，但这些来源缺少足以确认内容类型的
证据，因此没有授权 SAME。`短篇`、`同人`、页数/epsCount、没有 CG 标签都不等于明示 manga。
本批不存在完整作者库存范围证书，也不能把 119 条无本地主标题字面对应的记录判作新作品。

生产能力的正例已由独立 fixtures 验证：明示类型+唯一完整身份可绑定 existing；确认完整的
同系列库存范围+不同单一明确编号可证明新作；人工 SAME/source ID 继续权威。真实 corpus 中
没有凭空补入上述前提。所有自动 existing/new evidence rule 在本批的授权数量均为 0：

| identity evidence rule | 授权数量 |
|---|---:|
| UNIQUE_EXACT_STRUCTURED_IDENTITY | 0 |
| COMPLETE_SCOPE_DISJOINT_EXPLICIT_INSTALLMENT | 0 |
| HUMAN_SAME | 0 |
| SAME_SITE_SOURCE_ID | 0 |

## Before / After

原 Phase 3B：UNRESOLVED_TITLE_IDENTITY 181、UNCONFIRMED_AUTHOR 28、UNKNOWN_LOCAL_VERSION 4、
LOW_INFORMATION_OR_COLLECTION_TITLE 2。原始分布直接读取已验收 export，未重做 M1 分析。

| M2 review reason | 数量 |
|---|---:|
| UNKNOWN_SOURCE_CONTENT_TYPE_NO_IDENTITY_PROOF | 107 |
| UNINTERPRETED_ANNOTATION | 48 |
| UNCONFIRMED_AUTHOR | 28 |
| TITLE_WITNESS_UNKNOWN_SOURCE_CONTENT_TYPE | 13 |
| UNPARSED_IDENTITY_MARKER | 7 |
| MALFORMED_BRACKETS | 5 |
| BILINGUAL_UNRESOLVED | 3 |
| LOW_INFORMATION_TITLE | 2 |
| NO_DETERMINISTIC_IDENTITY_OR_NEW_WORK_PROOF | 2 |
| **总计** | **215** |

`before-after.json` 包含每个 source key 的原 reason 和完整 M2 Outcome，含 author evidence、
原文/normalized/core、被分离 metadata 和 provenance、全部结构字段、类型证据、每个本地候选
的 identity/relation、真正的标题候选列表、唯一性判断和下游状态。没有把 scope_work_ids 当 SAME 候选。
M1 的六个既有诊断分组在新结果中的映射计数见 `verification.json`；其中 119 条全部仍 review。

13 条仅有标题线索、尚未授权的候选：

| source key | work_id |
|---|---|
| jm:1016608 | WORK_01167 |
| jm:1048382 | WORK_01169 |
| jm:1050136 | WORK_01175 |
| jm:1444812 | WORK_00799 |
| jm:1449300 | WORK_02657 |
| jm:234503 | WORK_00098 |
| jm:235554 | WORK_00098 |
| jm:623646 | WORK_01167 |
| pica:5eecb90cd4e4dd51499aa482 | WORK_00097 |
| pica:5ef4e9131b877a1ddde7d357 | WORK_00097 |
| pica:60118765f8d9774a09d46a66 | WORK_00098 |
| pica:66bcce2a41a5931b3b878cb9 | WORK_01172 |
| pica:692f2e2a2439ee5832b5114a | WORK_00799 |

原来的 4 条 UNKNOWN_LOCAL_VERSION 都在这 13 条中：M2 在身份层先阻断未知 source type，
不继续沿用旧 matcher 默认 manga 的自动绑定。这 4 条不是人工 SAME 或同站 source mapping。

## 空 primary 修复

`LOCAL_ITEM_2705 / WORK_02657` 在正式 2833 inventory 中保留了完整文件名，作者已确认为 2-G。
原 null 原因是旧 parser 缺 creator anchor。新增独立修复夹具记录原 item、原 inventory SHA-256、
行号 2705、work/local IDs、expected before/after，恢复 primary 和 `(オリジナル)` fandom。
只有 staging 副本应用修复；原 monitor-state 和完整作者名单未改动。

受影响的原 17 条记录不再受 null primary 阻断：1 条形成标题线索但 source type UNKNOWN，
13 条 source type UNKNOWN 且无完整身份对应证明，3 条仍有未解释 annotation；全部继续 review。
修复重复应用不产生变化，任何不同的现有标题、作者/local IDs 都会拒绝覆盖。

## Windows / Linux / 幂等与证据一致性

| 项目 | 结果 |
|---|---|
| 原有 Windows tests | **117/117 保留 PASS** |
| 新增 M2 tests | **53/53 PASS** |
| Windows workspace | **170/170 PASS** |
| Linux workspace | **170/170 PASS** |
| Linux Actions run | **34011408700 — success** |
| 两平台第二次相同 replay | reanalyzed=0，new_events=0，business_state_unchanged=true |
| 两次 checkpoint | 同平台字节完全一致 |
| Windows/Linux before-after | 字节完全一致 |
| 源站请求 / 图片请求 / 漫画下载 / 删除许可 | **0 / 0 / 0 / false** |

Actions：[Matcher M2 offline validation](https://github.com/gouluanjiang/MangaMonitor/actions/runs/34011408700)。
测试代码 SHA 与完整 jobs/steps 状态保存于 `evidence/matcher-m2/linux-run.json`。
跨平台 before-after SHA-256：
`7aa6498b734abe49867367b96c89f03f93248d3e0eb0e2c28b87ba08887192b4`。

53 项新测试由 31 项 parser、20 项生产身份/状态规则、2 项 CLI 构成；防误判覆盖包括：
24 条既有 M1 adversarial cases 全部在新 matcher 上重跑、11 条 wrapper 碰撞反例、9 条普通
正文边界样本，以及未知/冲突类型、重名候选、隐藏 null/未知 fandom 碰撞、不同标题不判新、
不完整/失效范围证书、前导零/范围/Extra 不判新、NOT_SAME/IGNORE、跨站 ID 不冒充权威等。
完整测试名及结果在两个平台的 tests.log 中。

Linux workflow 仅手动触发，无源站凭据或扫描步骤；replay 时 HTTP/HTTPS/ALL_PROXY 均设为
不可用的 loopback 地址。binary 无 adapter/client 分支，拒绝 live-source 选项，强制 production
配置为 false，拒绝写入 seed 或覆盖已有输出。原采集 tape 已在本地，不需要请求 JM/Pica。

## 修改文件与交付

- `MATCHER_M2.md`：语法、闭集规则、provenance、碰撞策略及受控接入边界。
- `MATCHER_M2_RESULTS.md`：本报告。
- `crates/rules-core/src/title_m2.rs`、`src/lib.rs`：生产身份语法与模块入口。
- `crates/rules-core/tests/matcher_m2.rs`：语法、碰撞、M1 adversarial 回归。
- `crates/cloud-monitor/src/matcher_m2.rs`、`src/lib.rs`：四类身份决策、可审计 repair 和 replay。
- `crates/cloud-monitor/src/monitor.rs`：仅抽出共享 bind_work，保留既有下游安全规则。
- `crates/cloud-monitor/src/bin/matcher-m2.rs`：受控离线入口、完整 export/tape 一致性校验。
- `crates/cloud-monitor/tests/matcher_m2.rs`、`matcher_m2_cli.rs`：身份/状态/输入保护及完整幂等测试。
- `fixtures/matcher-m2/`：已有 Phase 3B observations 与独立库存修复证据。
- `.github/workflows/matcher-m2.yml`：Linux 全套测试与离线双 replay。
- `evidence/matcher-m2/`：逐条 before/after、Windows/Linux 首次/第二次摘要、测试日志、run 元数据、交叉验证结果。

未新增依赖或改动 Cargo.lock。production_enabled=false；monitor-state 仍为五作者 seed。
未调用旧生产扫描入口，未导入全部作者，未做漫画/图片下载、exe、自动替换或删除。
未批准的 wrapper、publication、双语、标题字形别名和不明结构仍 review；本轮不补猜测证据。
**M2 到此停止，等待 ChatGPT 正式验收，不进入 M3。**
