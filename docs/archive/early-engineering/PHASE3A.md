# Phase 3A 使用与交付边界

从已验收的 Phase 1A 提交 `752a511` 继续，保留原工作区、适配器和全部旧测试。正式运行无需 Python 或 AI。没有漫画下载、图片下载、文件删除或 updater。

## 手动运行与恢复

```powershell
. .\scripts\Use-LocalTools.ps1
cargo test --workspace --locked
cargo build -p cloud-monitor --bin phase3a --locked
# 在环境变量配置 PICA_TOKEN，或 PICA_EMAIL/PICA_PASSWORD 后运行。
.\target\debug\phase3a.exe --state fixtures/phase3a-seed --output reports/first --authors fixtures/phase3a-authors.json --mode full --max-requests 400
# 只回放上一轮的脱敏输入，不再请求源站：
.\target\debug\phase3a.exe --state reports/first --output reports/second --authors fixtures/phase3a-authors.json --mode full --replay reports/first/observations.json --assert-idempotent
# 预算中断后：保持原 state/output/作者/mode/threshold，增加 --resume。
.\target\debug\phase3a.exe --state fixtures/phase3a-seed --output reports/first --authors fixtures/phase3a-authors.json --mode full --max-requests 400 --resume
```

CLI 强制 5–10 个不重复、已启用的确认作者。输出必须与输入目录分离，不能覆盖输入 state。默认仅写 artifact 目录，无正式状态提交开关。请求预算是单次进程预算，恢复后重新计算；每次真实请求沿用适配器的 1–3 秒随机间隔。

手动 Actions：`.github/workflows/phase3a.yml`。默认真实只读采集五个作者，然后同输入回放及增量回放。可指定 `evidence_run` 为之前 Phase 3A 运行 ID，只下载私有产物并用当前代码重放，不额外访问漫画来源。没有月度 cron，也没有全名单扫描入口。workflow 只有仓库内容和 Actions 的只读权限。

## 八个状态文件

输入支持交接 seed，输出使用版本 2 的检测结构，库存本身的版本和 ID 不重写：

| 文件 | 本阶段内容 |
|---|---|
| authors.json | 保留输入作者与 enabled，不从搜索结果扩充 |
| catalog.json | records：原始来源身份、搜索/详情分离指纹、时间、分析次数、work_id、可用性连续检查计数 |
| inventory_index.json | 原样保留传入的库存镜像，不触碰本地仓库 |
| pending.json | 每个 work_id 一个 task，保留 task_id/first_seen，候选变化增加 revision |
| review.json | 可读的来源、原作者字段、标题、原因、候选 work IDs；保留历史 cleanup_review |
| decisions.json | positive_mappings、negative_mappings、ignored_source_records、ignored_works |
| scan_state.json | phase3a_scan：开始前历史 ID 快照、逐作者/来源游标、扫描模式、完整边界、上次 full 时间、通知去重键 |
| latest.json | 当前这一轮新事件，不重复列出历史 pending/review |

额外的 `checkpoint.json` 是单文件原子替换的恢复快照；八个 JSON 是该快照的可读导出。`--resume` 使用快照，防止中断时读取到混合代次。普通新扫描读取八个 JSON，因此新的人工作业可以修改 decisions 后生效。恢复旧扫描期间不同时修改导出文件；要纳入新决定，应开始新扫描。

保留三个身份：来源键 `jm:123` / `pica:...`，抽象 `work_id`，库存版本引用 `local_item_id`。从源站新发现且确认缺失的抽象 ID 使用稳定 `WORK_SRC_...` 前缀，不重编号交接库存。

人工决定格式示例（示意值，不是实际漫画）：

```json
{
  "schema_version": 2,
  "positive_mappings": [{"source_key": "jm:123", "work_id": "WORK_00001"}],
  "negative_mappings": [{"source_key": "pica:0123456789abcdef01234567", "work_id": "WORK_00002"}],
  "ignored_source_records": ["jm:456"],
  "ignored_works": []
}
```

SAME、NOT_SAME 和 IGNORE 会使分析上下文变化，在新扫描开始、网络请求之前生效。冲突的正向映射进入人工检查，不覆盖冲突。当前没有人工审核 GUI。

## 扫描与错误

full 不按历史 ID 早停；incremental 使用扫描开始前存在的来源 ID，默认连续阈值仍是 5。处理完已取得的整页再停止后续分页，记录 `EARLY_STOP_HEURISTIC`，避免把已经取得的页描述为未访问。若这一页本身已是可靠末页，则记录 COMPLETE。首次或距该作者/来源上次 full 满六个月，自动选恢复型 full。

各作者/来源单独保存 next_page、已观察 ID 和连续历史计数。网络/认证错误、预算不足、分页没有进展都留下 partial/checkpoint；不会通过未出现推断下架。200 作者仍只是未来长跑的软批次概念，本阶段不运行 200 人，也没有新增分布式协调组件。

pending 和元数据明确 `finished=false` 的旧记录可以按 ID 主动复查。发现指纹变化重新抓详情并分析；搜索指纹与详情指纹分开存储，避免字段结构不同导致每次都误报变化。直接复查也比较详情指纹。

不可用计数仅接受 `ExplicitUnavailable`：同一 check_id 不重复计数，连续三次才使来源及对应候选 inactive；成功详情重置计数，可恢复 inactive 的任务。普通 HTTP 404、认证、网络和解析错误不计数。

Pica 已验证详情路由返回的严格组合：HTTP 404 + JSON code 404 + error 1007 + message `not found`，仅该组合归为明确不可用。JM 的试探响应没有明确的记录不存在证明，所以当前 JM 错误继续保守返回 source_error；**没有宣称已经实现可靠的 JM 自动下架识别**。这不影响查缺和 review，且避免错误清空 pending。不是通过连续空搜索或早停进行兜底判断。

## 匹配与版本的实际范围

已确认来源映射优先，已知同站 ID 保持作品身份。自动跨站/本地合并采用已确认单一作者 + 保守标题一致；标准化只做 Unicode/大小写和空白，不删数字、标点、连字符、Part 或前后篇。没有复用 Phase 2A 中会消去连字符的兼容 key。

缺作者、社团/多人组合、低信息标题、合集范围及身份歧义都进入 review。标题无法确定对应且已有同作者候选时，不通过翻译语义猜测缺失。这会产生较多人工检查项，是本阶段的保守取舍。

版本只解析明确词证据，无证据或冲突证据保持 UNKNOWN；不会把汉字存在本身当作汉化证明。候选选择复用中文、无码、全彩、人工偏好、可靠大小、JM 的顺序。API 没有可靠整本大小，本阶段传 None，不为大小下载。候选选择与已有库存自动升级使用不同规则。

内容覆盖暂不从自由文本自动建立：未知 coverage 为 null，合集等进入 review。已有覆盖谓词和删除安全纯判定保留测试，**没有删除授权或删除执行**。自动跨卷覆盖、完整生产标题剥离规则和手工覆盖编辑工具仍未实现。

## 本阶段输出

每次输出八个状态文件、checkpoint、`observations.json`、`scan-report.json` 和 `state-diff.json`。报告区分实际请求和 replay、是否完整、分析数量、业务状态是否变化、事件和各来源边界。`last_seen/last_checked` 等观察时间更新不算业务幂等失败。

首次真实采集与后续当前代码回放分别记录 commit/run ID。最终数字、Linux 运行链接、测试结果和样例见 `PHASE3A_RESULTS.md`。

Phase 3B 前仍需冻结最终作者名单。本包的 653 人 preview 和 canonicalization patch 没有应用到正式状态。下载器、漫画更新.exe、删除、正式全作者扫描、正式 state 自动推送和生产月度调度均未开始。
