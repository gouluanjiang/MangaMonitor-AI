# Phase 1A 实测交付（2026-09-06，北京时间）

## 结论与边界

JM 和 Pica 在本地 Windows、GitHub 托管 Linux runner 上均完成本次只读验证，当前设计没有被实测证明不可行。Pica 本地分别验证了现有 token 和账号密码登录；Linux 验证了账号密码登录。正式运行不使用 AI。

这是三个作者的小规模联网证据，不代表长期可用性、所有作者完整性或生产匹配正确性。没有下载图片或漫画，没有修改漫画仓库，没有实现自动下载、删除、月度扫描或库存回写。交接标题中的“start of Phase 3”未扩大本次用户明确限定的 Phase 1A 范围。

先恢复现有工作区并推送已有代码，再从中断位置运行 Linux 验证，没有重新初始化 Phase 1A。实测代码提交：`8d1c846464057502590df4844176ec240d772ec7`，前置提交 `9a2dc12`。

- 私有仓库：<https://github.com/gouluanjiang/MangaMonitor>
- Linux 运行：<https://github.com/gouluanjiang/MangaMonitor/actions/runs/33982043826>
- Linux job：`101348888701`；测试、编译、联网验证及产物上传全部成功。
- 上游 JM：`f0cdd724af6892002f2fb7be883b88832cebe7e9`
- 上游 Pica：`77c8b62ede42b3afc074506d092313816af8092d`

## 真实请求与耗时

作者名称直接取自确认名单：`40010試作型`、`Hisasi`、`武田弘光`。每个真实 API 请求前随机等待 1–3 秒，串行，无隐藏重试、无重定向。总耗时包含等待，不包含编译、安装工具和 GitHub 管理 API。登录也计入请求数。

| 运行 | JM 请求 / 耗时 | Pica 请求 / 耗时 | 整轮耗时 | 结果 |
|---|---:|---:|---:|---|
| Windows 首轮，最多 3 页 | 16 / 39.244 秒 | 18 / 46.680 秒，现有 token | 85.941 秒 | 两站通过；Pica 两个作者达到采样上限 |
| Windows 单独账号密码验证 | — | 19 / 48.065 秒，含 1 次登录 | 48.071 秒 | 登录及后续搜索详情通过 |
| Windows JM 日期与缺作者详情补核 | 16 / 43.279 秒（整轮） | — | 43.279 秒 | 通过，发现排序及作者字段限制 |
| Linux 最终验证，最多 5 页 | 16 / 33.506 秒 | 22 / 50.721 秒，含 1 次登录 | 84.233 秒 | 两站所有样本搜索到末页 |

以上四轮合计 107 次源站 API 请求，图片请求为 0。Linux 整个 job 约 141 秒，包含环境准备与编译；不可与 API 时间混用。

每个作者另取 2 条详情并重复请求第一页。后续补核优先选择作者缺失或非精确作者字符串的记录。JM 各作者请求数为 6、5、5；Linux Pica 为 7、6、8，另加一次登录。

## 分页、ID 与搜索行为

| 来源 | 作者 | 最终逐页记录数 | 唯一 ID 数 |
|---|---|---|---:|
| JM | 40010試作型 | 80、80、23 | 183 |
| JM | Hisasi | 80、13 | 93 |
| JM | 武田弘光 | 80、71 | 151 |
| Pica | 40010試作型 | 20、20、20、2 | 62 |
| Pica | Hisasi | 20、20、16 | 56 |
| Pica | 武田弘光 | 20、20、20、20、7 | 87 |

Linux 六组搜索均 `exhausted=true`，跨页没有重复 ID，重复第一页的 ID 与顺序均一致，12 次详情的 ID 均与搜索记录一致。Windows JM 同样到末页；Windows Pica 首轮只读取最多三页，不能把那轮的 PASS 描述为所有作者全量完成。

JM 使用 `main_tag=0`、作者关键词、`page`、`o=mr`；Pica 使用 advanced-search、作者关键词及 `sort=dd`。这是作者名关键词搜索，不是源站保证的精确作者过滤。

JM 数字 ID 与 Pica 24 位十六进制 ID 保持字符串形式，并带来源组成身份。重复请求证明本次观察的一致性，不证明跨月永久不变，也不代表两站记录属于同一抽象作品。

## “最新”排序实测

统计搜索顺序中后一个时间大于前一个时间的相邻逆序次数：

| 来源 / 字段 | 40010試作型 | Hisasi | 武田弘光 | 判断 |
|---|---:|---:|---:|---|
| JM `adddate` | 7 | 1 | 2 | 非严格降序 |
| JM `update_at` | 63 | 34 | 38 | 非严格降序 |
| Pica `updated_at` | 0 | 0 | 0 | 本次全部符合非递增顺序 |
| Pica `created_at` | 5 | 2 | 13 | 非严格降序 |

JM 的 `mr` 不能当作严格日期游标，原因尚未确定，不能擅自归因为置顶或服务端缓存。Pica 的 `dd` 在样本中符合更新时间顺序，并非严格创建时间顺序；这仍是观测结果，不是接口长期契约。

因此保留交接冻结的“连续历史 ID 阈值 5，可配置”和“每六个月全分页恢复扫描”，但必须把早停视为可能延迟发现的启发式策略。未来扫描应记录早停原因、完整/部分状态，不能将早停后的未出现判为下架。没有借此重设阈值或实现新扫描架构。

## 实际字段与作者限制

JM 搜索字段名：`adddate, author, category, category_sub, description, id, image, is_favorite, liked, name, update_at`。

JM 详情字段名：`actors, addtime, author, comment_total, description, id, images, is_aids, is_favorite, liked, likes, name, price, purchased, real_link, related_list, series, series_id, tags, total_photos, total_views, works`。

Pica 搜索字段名：`_id, author, categories, chineseTeam, created_at, description, finished, likesCount, tags, thumb, title, updated_at`。

Pica 详情字段名：`_creator, _id, allowComment, allowDownload, author, categories, chineseTeam, commentsCount, created_at, description, epsCount, finished, isFavourite, isLiked, likesCount, pagesCount, tags, thumb, title, totalComments, totalLikes, totalViews, updated_at, viewsCount`。

以上是可见字段名清单，不表示保存了每个字段的值。报告只保留白名单元数据与字段名，不保存图片地址、上传者个人资料、完整响应或认证头。

JM 427 条搜索记录中，57 条作者字段为空：三个作者分别 23、8、26 条。详情补核 `137815`、`90539` 也仍为空，因此“补一次详情必能取得作者”不成立。精确等于查询作者的记录分别为 104、46、89 条；其余不能简单认定为误报。

Pica 最终 205 条搜索结果没有空作者，但精确等于查询名称的仅为 25、26、38 条。例如 `40010壱号 (40010試作型)`、`ねこはまんまがうつくしい (Hisasi)`、`しゅにち関数 (しゅにち, 武田弘光)`，也有 `多人`。保留原始字段；不能将社团、多作者组合直接加入作者名单，也不能把查询词当成已经确认的作者。缺乏确定证据的匹配继续人工检查，不增加别名搜索。

两站均未在本次元数据中提供可直接用于平局选择的可靠总文件字节数；页数不等于文件大小。中文、无码、全彩也没有统一可靠的三项布尔字段，标题、标签和汉化组字段只提供规则证据，没有证据保持 UNKNOWN。不得为了大小比较下载文件。

## 认证与环境限制

JM 本次使用已有配置对应的 `www.cdnhth.cc`，搜索和详情无需账号登录；未遍历所有备用域名。Pica 使用固定上游 API 协议：本地现有 token 成功，独立账号密码登录成功，Linux 账号密码登录及随后搜索详情也成功。未测试错误密码、token 有效期、跨设备会话互斥或长期登录频率限制；本轮未遇到验证码或限流，不代表以后没有。

GitHub 凭证留在本机隔离 CLI 配置，Pica 凭证使用进程环境或 Actions Secrets。没有将账号密码、个人 token 写进代码、报告、命令参数或仓库。已完成私有仓库创建和 workflow 权限补充；workflow 运行自身仅需要 `contents: read`，不写库存。

Windows 使用工作区内便携 Rust 1.98.1 GNU 工具链和 w64devkit；初次构建发现 GNU 辅助工具及链接库路径问题，已经通过本地环境脚本解决，未修改系统 PATH。Linux 使用 Rust 1.98.1 和锁定依赖，构建成功。短时成功不足以推断 666 个作者长跑耗时；每 200 作者仍只是软批次，首次全量及状态断点恢复尚未实施或压测。

## Rust workspace 与规则验证

```text
Cargo.toml / Cargo.lock
crates/
  state-model/     local_item_id、work_id、source key 分离；UNKNOWN；元数据指纹
  rules-core/      无网络和文件操作的确定性规则
  jm-adapter/      固定上游版本的 HTTP 搜索与详情协议
  pica-adapter/    登录、搜索、详情及章节元数据接口
  cloud-monitor/  本阶段有页数上限的只读测试 CLI
fixtures/         原始规则 JSON、配置及三个作者名称
scripts/          本地工具、只读验证、隐藏输入登录辅助脚本
.github/workflows/phase1a.yml  仅手动触发
third-party/      上游版本说明和许可证
```

适配器复用上游协议逻辑，剥离 GUI/AppHandle 依赖，没有修改原下载器。Pica 章节元数据方法遇到不完整分页会失败；本轮联网 smoke 验证的是搜索分页及详情，并未实际调用章节枚举，不能把该方法标为已联网验证。现有 GUI 下载命令只入队的行为已记录，未包装为“下载完成”。

原始 40 个规则夹具全部迁移为独立命名 Rust 测试，JSON 与交接文件 SHA256 比对一致，未修改预期来适配实现：

| 规则组 | 数量 | Windows / Linux |
|---|---:|---|
| 候选选择 | 6 | 全部通过 |
| 自动升级判定 | 10 | 全部通过 |
| 内容覆盖 | 8 | 全部通过 |
| 任务完成与 revision | 3 | 全部通过 |
| catalog 变化 | 5 | 全部通过 |
| 删除前覆盖保留纯判定 | 3 | 全部通过 |
| 标准化 | 5 | 全部通过 |

另有 3 个规则保护用例和 6 个适配器/CLI 测试，两端各 49 项全部通过。Windows `cargo clippy --workspace --all-targets -- -D warnings` 通过，代码已格式化。Python 参考程序不参与 Rust 正式运行。

删除相关测试仅验证纯逻辑谓词，没有文件删除能力，也不能充当删除授权。原参考标准化会消去连字符，兼容测试通过不意味着可单凭该 key 认定身份；生产匹配仍必须保留并比较范围和结构。当前未实现完整自动匹配引擎。

交接库存及状态原样保存在忽略上传的 `state/`，没有重新提取作者或改变 OWNED。未重新引入 seen.json；后续继续使用 catalog、decisions 和 scan_state 等交接模型。

## 证据位置与下一步

工作区 `reports/` 保留：

- `local-smoke.json`：首轮两站只读请求。
- `local-pica-login.json`：本地账号密码登录验证。
- `local-jm-adddate.json`：JM 排序及缺作者详情补核。
- `rust-tests-local.log`、`clippy-local.log`、`fixture-integrity.json`。
- `linux-run-33982043826/phase1a-evidence-33982043826/linux-smoke.json` 和 `rust-tests-linux.log`：已从成功 Actions 运行下载。

GitHub 私有运行产物名 `phase1a-evidence-33982043826`，保留期 14 天；本地副本不依赖该到期时间。原始报告不提交到 Git，本文汇总可长期留存。

Phase 1A 要求的本地/云端可用性和全部规则夹具已验证。下一阶段如继续，应按既有设计实现只检测的 catalog、任务与检查点，重点兑现排序早停的限制、作者缺失人工检查及同 ID 指纹变化处理。章节枚举联网验证应留到确实需要该接口时完成。此交付停在 Phase 1A，不启用下载或替换。
