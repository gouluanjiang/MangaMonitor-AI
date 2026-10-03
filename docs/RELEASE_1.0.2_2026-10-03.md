# MangaMonitor 1.0.2 发行记录

状态：2026-10-03。1.0.2 已公开；原桌面入口已更新并核对实际进程；本机交付方式与未测范围如下。公开发行包不含私人资料或漫画。

## 源码与发行身份

- PR [#28](https://github.com/gouluanjiang/MangaMonitor-AI/pull/28) 已合并，收录 PR #23—#27 的累积改动；GitHub 已将这些祖先 PR 标为合并。
- 审阅与构建来源：`2a6aa1faef5b4353b555a97cfc19a0b8847c087f`。
- PR 构建 checkout：`d991753ca1e52b4a76d450167ff88375ce5e8d5d`。
- 实际 main 合并与 `v1.0.2` 标签：`457e6fef9ceebaa220ebcb0223da30de7be00c9c`。
- 三者的源码树均为 `76732cebfefe1e03b5eb9f4093fd86f249975217`；保留不同提交角色，不混称同一 SHA。

相对最后安装的 `068b66b` 候选，发布准备只更新版本元数据与交付文档，没有加入新的运行时逻辑、数据协议或依赖。发布工作使用独立工作树，原开发分支未切换或覆盖。

## CI 与产物核验

| 检查 | 发布源码 CI | 实际 main 合并 CI | 结果 |
| --- | --- | --- | --- |
| Rust 基线 | [37105964415](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/37105964415) | [37108202681](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/37108202681) | 均通过 |
| 前端逻辑／浏览器 | [37105964345](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/37105964345) | [37108202662](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/37108202662) | 均通过；362 逻辑、269 浏览器 |
| Windows 原生／安装 | [37105964406](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/37105964406) | [37108202688](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/37108202688) | 均通过；原生、Clippy、NSIS、安装后 WebView／重启／资料保留 |

发行使用发布源码 Windows CI 的 artifact `11268377652`。下载后检查 ZIP CRC、manifest 身份、全部 8 项载荷的字节数与 SHA-256，以及 EXE 的 x64 架构和 1.0.2 版本资源。先在草稿核验，再于 2026-10-03 公开；公开资产再次回下载核对，latest 和标签也已验证。合并后 CI 作为补充证据，不替换已发布字节。

## 公开发行资产

[正式发行页](https://github.com/gouluanjiang/MangaMonitor-AI/releases/tag/v1.0.2)，发行 ID `402400688`；非预发行，原 1.0.0／1.0.1 保留。

| 文件 | SHA-256 |
| --- | --- |
| `MangaMonitor-1.0.2-windows-x64.zip` | `265b81f70c4b4748d991a1dccffd2dd78102aa7b041e604a838a2f1de5c9a58f` |
| `MangaMonitor.Dev_1.0.2_x64-setup.exe` | `355ded6e740c0159712b2fdc031c94517fe1cd541facf865b86cbd1986a7724e` |
| `manifest.json` | `2fa09c08fe9b35ac2d9afd79de8103313f541e7d6d28f5f210772de9693cf5d2` |
| `SHA256SUMS.txt` | `95b47f339f902fbe97a207d1165dd1dcddb1717dfff763d07052911891d5e075` |

最终安装主 EXE：`b27b58733dcfa6f081497e1ffbcf2253f2bc55b01828d7ecca993a4341894339`。完整许可资源与文档哈希见公开 manifest。

## 本机交付

2026-10-03，用户再次明确要求替换桌面程序。开始更新前应用已退出；先保存原安装、快捷方式和最新资料的可恢复备份。

本机 NSIS 首次尝试在解包前报“Error writing temporary file”，返回 2；普通临时目录的合成写入探针正常，具体环境原因尚未定因。单独指定本次进程临时目录后返回 0，但目标文件及登记仍是旧版，哈希检查正确阻止了“安装成功”的误报。因此最终采用公开发行包中已经过 CI 安装验证的七项载荷，在原安装位置逐文件原子替换；没有把 NSIS 尝试写成成功。

七项最终文件均与 manifest 的字节数和 SHA-256 一致；现有安装登记版本为 1.0.2，桌面入口仍指向原位置。保留原卸载器，其管理的程序／资源路径清单没有改变。应用 ID、凭据命名空间和资料目录保留；没有更改 ACL、全局环境变量或安全防护。

17:02（UTC+08:00），从原桌面快捷方式启动。实际进程路径与最终 EXE 哈希匹配，进程持续响应。23 份资料文件在安装前后、首次启动前字节相同；启动后漫画库、下载、普通关注、历史、阅读进度和作者查询策略六类文件仍字节相同。作者目录的全部作品身份和来源范围保留，启动造成的缓存补充、详情标签替换及特别关注状态更新另记；没有声称全部目录文件启动后仍逐字相同。已识别的合成测试文件与身份均无残留。

窗口工具本轮持续报告“Computer Use helper already has an active request”，重连与内核重置未恢复，故未完成本次安装后的原生画面逐页检查；进程和资料核对不代替用户界面验收。临时目录问题只作安装环境问题留档，没有为本次交付改应用代码或绕过系统安全提示。

私有安装、快捷方式、资料备份与核对凭据保留在仓库外。公开文档不包含个人路径、账户、书目或凭据。

## 验证边界与后续

- Windows 本地专项最终范围为 **28 PASS、0 FAIL、0 BLOCKED、8 WAIVED、1 UNTESTED**；各项保留实际测试提交和层次，不重记为最终正式包上的 37 项原生通过。
- 用户取消 R05、D02、P01、P02、P03、P04、B03、B05 的剩余测试；B06 真实自然特别关注提醒留待用户日常体验。发布不会恢复取消项。
- LOCAL-01 最近更新元数据回写和 LOCAL-02 占用文件拒绝回收后的库存状态已修复并定向复测。
- LOCAL-03：CI 冷启动最近更新页曾观察到 15 像素滚动偏差。同源码重跑及本次正式 CI 通过，没有证明其根因已修复，仍保留追踪。
- 已识别的合成测试漫画及测试登记已移出／清理，安装前再次核对无残留。不为发布进行全量作者扫描，也没有下载或删除真实漫画。
- 安装名保留 MangaMonitor Dev；应用 ID、资料目录及凭据命名空间沿用旧值。格式版本 2 资料不应无备份交给旧程序打开。Windows 包尚未签名，系统安全提示由用户自行处理，不改变防护配置。
- 云端自动监控不启用，`production_enabled=false` 保持原边界。后续按用户实际体验进入维护，不擅自重启取消的功能或整体 UI 重设计。

本记录及入口文档的后续提交仅补记交付事实，不重新构建、覆盖公开资产或移动发行标签。
