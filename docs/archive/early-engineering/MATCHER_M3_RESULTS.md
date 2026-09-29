# Matcher M3 最终验收报告

状态：M3 production identity 接线、旧 state migration、离线双 replay、Windows 与 Linux 回归
均已完成；本阶段完成后停止，不进入 M4。

## 生产接线与迁移结论

- production `State::analyze` 已只使用共享 M2 matcher core，版本 `matcher-m2-v1`。
- 旧 Phase 3B state 第一次运行：215 条全部重新分析；0 AUTO_EXISTING、0 PROVEN_NEW、
  215 REVIEW_REQUIRED。
- review migration：187 条 reason/review ID reclassification，28 条原 `UNCONFIRMED_AUTHOR`
  分类保留但迁移到新 evidence schema；独立审计合计 215，`latest.json` 用户事件为 0，
  重复 NEW_REVIEW 为 0。
- 13 条 title witness 仍全部被 UNKNOWN source content type 阻断。
- 第二次 production replay：reanalyzed=0、new_events=0、business_state_unchanged=true。
- repair overlay 严格验证完整 inventory before/after hash 与行级 contract，重复应用幂等，
  seed state 未修改；同一旧 state 的八个持久化文件已固化为跨平台只读 fixture。

## 215 条 production-path after reason

| reason | count |
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

## 回归与 production gate

- Windows workspace：174/174 PASS（原 170 项保留，新增 4 项 M3 integration/migration/repair tests）。
- Linux workspace：174/174 PASS；manual-only Actions run
  [34025184609](https://github.com/gouluanjiang/MangaMonitor/actions/runs/34025184609) SUCCESS，
  验证 commit `426a0a41c62831cc1143271d3a29c6719de0c74b`。完整回归与生产路径双 replay
  两个步骤均通过，staging evidence artifact 已上传。
- 首次 run `34024697464` 暴露出测试引用了被 gitignore 的本地 `reports/` 路径；没有 matcher
  assertion 失败。随后将原 Phase 3B 八文件 state 逐字节固化为 tracked fixture，并由上述
  successful run 在 fresh Linux checkout 验证。
- offline replay 的 source requests / image requests / downloads / deletion authorization：
  0 / 0 / 0 / false。
- `production_enabled=false`；五作者 seed、historical threshold=5、6 个月 recovery full、
  full/incremental、checkpoint/resume、soft batch、commit-aware update、latest window、pending revision、
  source reactivation 与 fail-closed unavailable 逻辑均由完整 workspace regression 保持通过。

## 留给 M4

- 215 条 review 仍需独立证据或人工 decisions；M3 不以 KPI 为目标降低门槛。
- source content type 为 UNKNOWN 的 13 条 witness 仍不能授权 SAME。
- inventory repair 仍是 staging overlay，是否永久写回正式 2833 原始库存留待后续正式库存阶段。
- production gate 仍关闭；全作者 registry、真实周期启用和任何下载/替换/删除均不属于 M3。
