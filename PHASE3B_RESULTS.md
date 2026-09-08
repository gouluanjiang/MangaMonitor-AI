# MangaMonitor Phase 3B 验证结果

## 结论

Phase 3B 的云端生产运行框架已实现并通过 Windows 与 GitHub Actions Linux
验证。生产开关仍为关闭状态，仓库中的 `monitor-state` 只包含 5 位作者的验证
种子；本阶段没有启动全部作者扫描，没有下载漫画或图片，没有开发本地 updater，
也没有执行删除。

真实来源验证使用作者 `10驛`、`2-G`、`3104`、`Bee導師`、`haruhisky`。
GitHub Actions Run `33998989019` 在提交
`d18cc4c15e41306f539aa5fe8899051f48604462` 上成功完成。

## 已实现的运行框架

- `phase3b` 使用独立 staging 目录，输入状态目录始终只读。每次分页、直接检查
  和最终完成都会原子更新 `checkpoint.json`，同时写入带 `base_commit` 和状态哈希的
  `state-manifest.json`。
- `monthly` 按 incremental 运行；每个作者/来源距上次 full 达 6 个月时，已有状态机
  自动切换成 recovery full。显式 `full` 始终忽略历史 ID 早停。
- 作者按配置的软批次处理，每批最多 200 位。每批成功或 partial 后都可形成独立状态
  提交；同一周期的 `latest.json` 事件跨批保留。
- `scripts/commit-monitor-state.sh` 只暂存 `monitor-state`。它在复制状态前后两次检查
  `origin/main` 是否仍等于扫描起点，发生竞态时返回 75，绝不 force push。
- `scripts/run-phase3b-cycle.sh` 根据 manifest 从下一批继续；若上批为 partial，则从已
  提交 checkpoint 恢复同一批。生产开关未打开时脚本拒绝运行。
- `pending`、`review`、`latest`、`decisions` 仍由同一确定性状态模型生成。人工修改
  decisions 或库存/作者上下文后，会在任何网络请求前重新分析现有 catalog。

## 作者证据规范化

仅加入了已明确允许的规则：

- Unicode NFKC 与大小写规范化；
- `社团 (正式监控作者)` 中完整、唯一的正式作者 token；
- `10驛` 与 `10駅` 的封闭字形等价表。

每个 catalog entry 新增 `author_evidence`，记录正式作者、规则和命中的原始 token；
`record.author` 原样保存。空 author、多原始 author 字段、括号外子串、拼写疑似错误、
同时命中多位正式作者都仍进入 review。规则不会修改或扩展 `authors.json`。

真实 215 条记录的作者证据分布：

| 来源 | DIRECT_EXACT | DIRECT_NORMALIZED | PARENTHESIZED_OFFICIAL_TOKEN | MULTIPLE_RAW_AUTHOR_FIELDS | NO_CONFIRMED_AUTHOR_EVIDENCE |
|---|---:|---:|---:|---:|---:|
| JM | 95 | 16 | 2 | 8 | 19 |
| Pica | 6 | 1 | 67 | 0 | 1 |

这次没有放宽标题 matcher。Phase 3A 的 114 条 `UNCONFIRMED_AUTHOR` 中，86 条因
明确作者证据进入下一层确定性检查；它们没有被强行匹配，主要转成标题身份 review。
review 总数仍为 215。

## 测试结果

Windows 和 Linux 均为 87 项 PASS：原 Phase 1A/2A/3A 的 79 项全部保留，新增 8 项
覆盖作者证据、JM unavailable 拒绝、软批次、monthly 映射、commit guard、partial
checkpoint/resume 和跨批 latest 事件窗口。

真实首次运行：

| 指标 | 结果 |
|---|---:|
| 作者 | 5 |
| HTTP 请求 | 228（JM 145，Pica 83） |
| 耗时 | 490859 ms |
| catalog | 215 |
| complete | true |
| source error | 0 |
| image request | 0 |
| pending | 0 |
| review | 215 |

请求间隔实测最小 1018 ms、最大 2979 ms、平均 2014 ms。JM 每位作者 1 个搜索页；
Pica 的 `3104` 和 `haruhisky` 为 2 页，其余为 1 页。10 个作者/来源边界全部为
`COMPLETE/full`。

第二次使用完全相同 observations replay：0 个网络请求、0 个新事件、0 条重新分析，
`business_state_unchanged=true`，catalog/pending/review 不变。

首次状态 diff 为 catalog 新增 215，pending 0→0，review 0→215，latest 新事件 215，
`deletion_authorized=false`。review 原因：

| 原因 | 总数 | JM | Pica |
|---|---:|---:|---:|
| UNRESOLVED_TITLE_IDENTITY | 181 | 113 | 68 |
| UNCONFIRMED_AUTHOR | 28 | 27 | 1 |
| UNKNOWN_LOCAL_VERSION | 4 | 0 | 4 |
| LOW_INFORMATION_OR_COLLECTION_TITLE | 2 | 0 | 2 |

## 失败与恢复验证

- 模拟 JM source error 后 checkpoint 为 partial，边界为 `SOURCE_ERROR`；inactive 为 0，
  unavailable streak 非零记录为 0。
- 用同一 checkpoint `--resume` 后 complete=true，inactive 仍为 0，unavailable streak
  仍全部为 0。
- 临时 Git 远端的正常状态提交成功；并发提交使旧扫描返回冲突码 75；从远端新 HEAD
  重新检出后恢复提交成功。测试提交 SHA 只属于临时仓库，见 evidence。
- 核心状态机进一步限制：只有 Pica 的已验证 not-found envelope 可以成为 unavailable
  证书；同一检查周期重试不重复累计，三个不同成功检查周期才 inactive。任何 JM
  unavailable 声明都按未认证失败处理。

## Evidence

完整的 sanitised review JSON/CSV、首次/第二次报告、状态 diff、作者证据与分页统计位于：

`evidence/phase3b-run-33998989019/`

## 仍未实现

- `monitor-config.json` 的生产开关尚未打开，正式完整作者名单尚未替换 5 作者验证种子，
  因此月度 schedule 当前只执行 gate 并跳过扫描。
- 没有启动全部作者 full scan。
- 没有进一步降低 `UNRESOLVED_TITLE_IDENTITY`；标题 matcher 的生产降噪留给后续 Astra
  验收阶段。
- 没有漫画下载、图片请求、本地 updater、自动替换或文件删除。
- Actions 的 Node 20 compatibility warning 来自当前 `actions/checkout@v4` 与
  `actions/upload-artifact@v4`，不影响本次通过结果；可在后续维护时随官方 action
  版本升级处理。
