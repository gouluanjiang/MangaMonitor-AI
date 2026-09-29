# 历史文档索引

这里保存已完成阶段的计划、审查和验收记录，便于追溯，不构成当前需求或执行授权。当前功能及边界以[项目状态](../PROJECT_STATUS.md)、[当前交接](../DEVELOPMENT_HANDOFF.md)和最新用户决定为准。

## 完整交接快照

- [截至 2026-09-29 的完整交接](DEVELOPMENT_HANDOFF_2026-09-29.md)。
- [截至 2026-09-15 的早期交接](DEVELOPMENT_HANDOFF_ARCHIVE_2026-09-15.md)。

## 旧仓库事项结项

2026-09-29 的仓库整理依据已有实现与当前产品范围关闭以下旧事项：

| 事项 | 关闭结论 |
| --- | --- |
| [#3：可信作者范围凭据](https://github.com/gouluanjiang/MangaMonitor-AI/issues/3) | `completed`：相应工程实现已完成 |
| [#4：仅新增库存应用](https://github.com/gouluanjiang/MangaMonitor-AI/issues/4) | `completed`：相应工程实现已完成 |
| [#5：验证库存后完成任务](https://github.com/gouluanjiang/MangaMonitor-AI/issues/5) | `completed`：相应工程实现已完成 |
| [#2：旧生产验收整改跟踪](https://github.com/gouluanjiang/MangaMonitor-AI/issues/2) | `not_planned`：旧云端生产范围不再作为桌面 V1 待办 |
| [#6：长期运行生产验收](https://github.com/gouluanjiang/MangaMonitor-AI/issues/6) | `not_planned`：常驻云端生产方案不在当前范围 |
| [#7：本地状态发布至云端](https://github.com/gouluanjiang/MangaMonitor-AI/issues/7) | `not_planned`：已取消的云端状态回传方案 |
| [#9：旧长期生产路线图](https://github.com/gouluanjiang/MangaMonitor-AI/issues/9) | `not_planned`：由已交付的桌面范围替代 |

上述工程结项**不是云端生产验收通过**，也不启用云监控；`production_enabled=false` 保持不变。16 个过期远端分支已在完整备份和恢复检查后删除；当前 `main`、历史 PR 和发行标签仍可追溯。私有备份与核对明细不进入公开仓库。

12 个历史工作流已停用，Rust 基线、桌面前端和 Windows 桌面这 3 个正式 CI 保持启用。历史工作流源码和已有运行记录保留，维护旧契约时可按明确范围单独重新启用；这不表示删除实现或完成云端生产验收。

## 早期工程计划与结果

下列 53 份文档原先位于仓库根目录，现统一保留在 `early-engineering/`。其中代码和命令里的路径仍以仓库根目录为基准；Markdown 链接已按新位置调整。旧 `V1`、`production`、`pending` 等阶段称谓不代表当前桌面正式版的完成状态，也不会开启云监控或恢复取消功能。

`PREPRODUCTION_AUTHOR_SEARCH_ACCEPTANCE.md` 保留旧云端生产门禁的历史依据；归档不代表云端生产获准启用。现行执行约束继续见 [AGENTS.md](../../AGENTS.md)。来源基线及独立仓库隔离约束仍保留在根目录的 [SOURCE_BASELINE.md](../../SOURCE_BASELINE.md)。

- [AI_IDEA_V1.md](early-engineering/AI_IDEA_V1.md)
- [ASSISTANT_A1_RESULTS.md](early-engineering/ASSISTANT_A1_RESULTS.md)
- [ASSISTANT_A2.md](early-engineering/ASSISTANT_A2.md)
- [ASSISTANT_A2_RESULTS.md](early-engineering/ASSISTANT_A2_RESULTS.md)
- [ASSISTANT_A3.md](early-engineering/ASSISTANT_A3.md)
- [ASSISTANT_A3_RESULTS.md](early-engineering/ASSISTANT_A3_RESULTS.md)
- [ASSISTANT_A4.md](early-engineering/ASSISTANT_A4.md)
- [ASSISTANT_A4_RESULTS.md](early-engineering/ASSISTANT_A4_RESULTS.md)
- [ASSISTANT_A5.md](early-engineering/ASSISTANT_A5.md)
- [ASSISTANT_A5_RESULTS.md](early-engineering/ASSISTANT_A5_RESULTS.md)
- [ASSISTANT_A6.md](early-engineering/ASSISTANT_A6.md)
- [ASSISTANT_A6_10.md](early-engineering/ASSISTANT_A6_10.md)
- [ASSISTANT_A6_10_RESULTS.md](early-engineering/ASSISTANT_A6_10_RESULTS.md)
- [ASSISTANT_A6_11.md](early-engineering/ASSISTANT_A6_11.md)
- [ASSISTANT_A6_11_RESULTS.md](early-engineering/ASSISTANT_A6_11_RESULTS.md)
- [ASSISTANT_A6_12.md](early-engineering/ASSISTANT_A6_12.md)
- [ASSISTANT_A6_12_RESULTS.md](early-engineering/ASSISTANT_A6_12_RESULTS.md)
- [ASSISTANT_A6_13_RESULTS.md](early-engineering/ASSISTANT_A6_13_RESULTS.md)
- [ASSISTANT_A6_14A_RESULTS.md](early-engineering/ASSISTANT_A6_14A_RESULTS.md)
- [ASSISTANT_A6_14B_RESULTS.md](early-engineering/ASSISTANT_A6_14B_RESULTS.md)
- [ASSISTANT_A6_15_RESULTS.md](early-engineering/ASSISTANT_A6_15_RESULTS.md)
- [ASSISTANT_A6_2_RESULTS.md](early-engineering/ASSISTANT_A6_2_RESULTS.md)
- [ASSISTANT_A6_3_RESULTS.md](early-engineering/ASSISTANT_A6_3_RESULTS.md)
- [ASSISTANT_A6_4_RESULTS.md](early-engineering/ASSISTANT_A6_4_RESULTS.md)
- [ASSISTANT_A6_5_RESULTS.md](early-engineering/ASSISTANT_A6_5_RESULTS.md)
- [ASSISTANT_A6_5_UPSTREAM_AUDIT.md](early-engineering/ASSISTANT_A6_5_UPSTREAM_AUDIT.md)
- [ASSISTANT_A6_6_RESULTS.md](early-engineering/ASSISTANT_A6_6_RESULTS.md)
- [ASSISTANT_A6_7.md](early-engineering/ASSISTANT_A6_7.md)
- [ASSISTANT_A6_7_RESULTS.md](early-engineering/ASSISTANT_A6_7_RESULTS.md)
- [ASSISTANT_A6_8.md](early-engineering/ASSISTANT_A6_8.md)
- [ASSISTANT_A6_8_RESULTS.md](early-engineering/ASSISTANT_A6_8_RESULTS.md)
- [ASSISTANT_A6_9.md](early-engineering/ASSISTANT_A6_9.md)
- [ASSISTANT_A6_9_RESULTS.md](early-engineering/ASSISTANT_A6_9_RESULTS.md)
- [ASSISTANT_DATA_CONTRACT_V1.md](early-engineering/ASSISTANT_DATA_CONTRACT_V1.md)
- [ASSISTANT_PHASE3B_SINGLE_AUTHOR_CONCURRENCY_RESULTS.md](early-engineering/ASSISTANT_PHASE3B_SINGLE_AUTHOR_CONCURRENCY_RESULTS.md)
- [ASSISTANT_V1_1_1_RESULTS.md](early-engineering/ASSISTANT_V1_1_1_RESULTS.md)
- [ASSISTANT_V1_1_RESULTS.md](early-engineering/ASSISTANT_V1_1_RESULTS.md)
- [ASSISTANT_V1_2_RESULTS.md](early-engineering/ASSISTANT_V1_2_RESULTS.md)
- [ASSISTANT_V1_3_RESULTS.md](early-engineering/ASSISTANT_V1_3_RESULTS.md)
- [ASSISTANT_V1_4_RESULTS.md](early-engineering/ASSISTANT_V1_4_RESULTS.md)
- [ASSISTANT_V1_5_RESULTS.md](early-engineering/ASSISTANT_V1_5_RESULTS.md)
- [ASSISTANT_V1_6_RESULTS.md](early-engineering/ASSISTANT_V1_6_RESULTS.md)
- [MATCHER_M1.md](early-engineering/MATCHER_M1.md)
- [MATCHER_M2.md](early-engineering/MATCHER_M2.md)
- [MATCHER_M2_RESULTS.md](early-engineering/MATCHER_M2_RESULTS.md)
- [MATCHER_M3.md](early-engineering/MATCHER_M3.md)
- [MATCHER_M3_RESULTS.md](early-engineering/MATCHER_M3_RESULTS.md)
- [PHASE1A_RESULTS.md](early-engineering/PHASE1A_RESULTS.md)
- [PHASE3A.md](early-engineering/PHASE3A.md)
- [PHASE3A_RESULTS.md](early-engineering/PHASE3A_RESULTS.md)
- [PHASE3B_RESULTS.md](early-engineering/PHASE3B_RESULTS.md)
- [PREPRODUCTION_AUTHOR_SEARCH_ACCEPTANCE.md](early-engineering/PREPRODUCTION_AUTHOR_SEARCH_ACCEPTANCE.md)
- [README-CLOUD-HISTORY.md](early-engineering/README-CLOUD-HISTORY.md)
