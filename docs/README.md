# 文档导航

当前正式版为 **1.0.2**。本页按用途组织入口；历史计划和阶段报告不代表当前待办。

1.0.2 已正式发布并完成原桌面入口升级：[版本说明](RELEASE_NOTES_1.0.2.md)、[实际发行记录](RELEASE_1.0.2_2026-10-03.md)。

## 使用与版本

- [使用指南](USER_GUIDE.md)：安装、账号、浏览、下载、阅读及问题诊断，适用于当前正式版。
- [当前项目状态](PROJECT_STATUS.md)：已交付功能、已知边界、取消项和以后讨论的范围。
- [1.0.2 版本说明](RELEASE_NOTES_1.0.2.md)与[正式发行页](https://github.com/gouluanjiang/MangaMonitor-AI/releases/tag/v1.0.2)。
- [1.0.2 发行记录](RELEASE_1.0.2_2026-10-03.md)：准确源码、CI、资产哈希、桌面安装及剩余验证边界。
- [1.0.1 版本说明](RELEASE_NOTES_1.0.1.md)与[正式发行页](https://github.com/gouluanjiang/MangaMonitor-AI/releases/tag/v1.0.1)。
- [1.0.1 发行记录](RELEASE_1.0.1_2026-09-29.md)：最终 CI、产物哈希、发布、安装及原生验收证据。
- [1.0.0 发行记录](RELEASE_1.0.0_2026-09-29.md)：首个正式版和全部作者检查的历史证据。

## 开发与维护

- [当前交接](DEVELOPMENT_HANDOFF.md)：接手开发先读这一份；不必顺序重读所有历史报告。
- [2026-10-02 合成长回归与实证修复](LONG_REGRESSION_2026-10-02.md)：准确基线、连续时长、性能对比、候选产物和 Windows 待验边界。
- [2026-09-30 维护审计修复](MAINTENANCE_AUDIT_2026-09-30.md)与 [1.0.2-rc.1 候选说明](RELEASE_NOTES_1.0.2-rc.1.md)：修复范围、格式兼容与待验收边界。
- [贡献和问题反馈](../CONTRIBUTING.md)、[开发规则](../AGENTS.md)、[来源基线与仓库隔离](../SOURCE_BASELINE.md)。
- [前端说明](../apps/local-workbench/README.md)与[原生桌面说明](../apps/local-workbench/src-tauri/README.md)。
- [许可](../LICENSE)与[第三方通知](../THIRD_PARTY_NOTICES.md)。

## 相关功能契约

按本次改动选择相关文档。下面是维护入口，不是待开发清单；后续用户确认和当前交接优先于旧文档的阶段状态。

| 改动范围 | 参考 |
| --- | --- |
| 下载执行及权限 | [下载执行解冻审查](DOWNLOAD_EXECUTOR_THAW_GATE.md)、[队列展示](DOWNLOAD_QUEUE_PRESENTATION_2026-09-26.md) |
| 作者检索与结果 | [查询规则](AUTHOR_QUERY_POLICIES_2026-09-22.md)、[逐条异常隔离](LISTING_ITEM_ISOLATION_2026-09-22.md)、[JM 分页边界](JM_SEARCH_BOUNDARY_2026-09-29.md) |
| 作者检查摘要 | [检查变化摘要](DISCOVERY_CHANGE_SUMMARY_2026-09-26.md) |
| 阅读 | [内置阅读器](BUILT_IN_READER_2026-09-26.md)、[独立阅读小窗](READER_WINDOWS_2026-09-27.md) |
| 账号与外观 | [会话与记住登录](SESSION_RESTORE_2026-09-27.md)、[整体 UI 集成](UI_INTEGRATION_2026-09-27.md) |
| 时间与语言标记 | [作品时间与排序](WORK_DATES_AND_SORTING_2026-09-20.md)、[来源语言标签](SOURCE_LANGUAGE_BADGES_2026-09-26.md) |

## 历史资料

[历史索引](archive/README.md)集中收录旧云端／助手／匹配器计划和完整交接快照。其他按日期命名的功能报告仍保留原路径，方便核对已存在的引用；其中过期的“当前”“待验收”“冻结”描述不覆盖现行状态。
