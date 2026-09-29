# Matcher M3 — 生产路径接线与状态迁移

基线：`main@2a57b8e068f05de04e90042c40947c3aa9badec7`。Matcher M2 的规则边界保持冻结，
规则版本继续使用 `matcher-m2-v1`；本阶段没有增加 wrapper、alias、简繁转换、模糊匹配或
语义推断。

## 正式生产 identity 路径

`State::analyze` 现在直接调用 `matcher_m2::decide`，离线 Matcher M2 replay 与 Phase 3B
monthly/full production runner 共享同一个确定性 identity core 和同一个 downstream
`bind_work`。旧 `core_title` 等函数只作为兼容/测试工具保留，不再参与 production work
identity 决策，也不存在第二套隐藏的自动绑定逻辑。

正式优先级仍为：IGNORE；人工 SAME/NOT_SAME；同站 source ID；M2 structured identity；
证据不足进入 REVIEW_REQUIRED。人工/同站 authority 的冲突和多映射继续 fail closed。
identity、版本选择、coverage、删除许可继续分层；SAME 不会授权自动升级、coverage 等价或删除。

## analysis context 与 schema migration

production analysis context 现在包含 `matcher-m2-v1`、decisions、inventory、authors。旧 Phase 3B
catalog 因缺少 matcher version context 会在第一次 M3 run 确定性重新分析；写回后相同输入的
第二次 run 不再重新分析。catalog/review 新增带 serde defaults 的 matcher version、identity
evidence 和 provenance，旧 schema 可直接读取，不要求删除 state。

规则升级触发的 active review 迁移保存在 review provenance 与独立的
`scan.review_migration` 审计汇总中：reason/review ID 改变计入 `reclassified`，分类不变但写入
M3 evidence/provenance 计入 `classification_unchanged`。迁移不会写入 `scan.events/latest.json`，
也不会产生重复 `NEW_REVIEW`。如果 source detail 真正变化并导致新的 identity condition，
则仍可生成新的 `NEW_REVIEW`。

## primary repair overlay

Phase 3B runner 可通过 `--repair-overlay` 在 staging inventory 应用已经验收的
`LOCAL_ITEM_2705 / WORK_02657` repair。production cycle script 已固定传入已提交 manifest。
overlay 同时校验完整 analysis inventory 的迁移前/迁移后 hash、local item、work、expected-before、
expected-after、作者、类型、文件名语法和 provenance；任一不匹配均 fail closed。重复应用结果
相同。正式 `monitor-state/inventory_index.json` 不被原地修改。

## 离线生产路径验收

手动 workflow `.github/workflows/matcher-m3.yml` 仅接受 workflow dispatch。它在无效
HTTP/HTTPS/ALL_PROXY 下使用真正的 `phase3b` binary，从已验收的旧 Phase 3B state 读取 215 条
catalog，并用已提交 observations 完成第一次 migration 和第二次 idempotent replay。workflow
使用受版本控制的 `fixtures/matcher-m3/phase3b-old-state` 作为该旧 state 的只读八文件副本，
确保 fresh checkout 与 Windows 使用相同输入；它还运行完整 workspace tests、核对输入 state
字节未变、production gate 关闭、无 source/image 请求，并上传 staging evidence。

production_enabled 保持 false；没有启动作者扫描、替换五作者 seed、访问 JM/Pica、下载漫画或
图片、实现 updater、自动升级/替换/删除，也未进入 M4。
