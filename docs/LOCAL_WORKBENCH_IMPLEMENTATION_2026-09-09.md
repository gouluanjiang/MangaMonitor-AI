# 已确认 UI 的第一批实现

日期：2026-09-09。用户接受剩余设计，要求删除作者头像、ZIP 打包及图像处理调节项，并明确本次及以后不再生成修改预览图。本记录区分界面实现与实际下载器交付。

## 本批范围和验收

- 作者关注只显示名字及来源/检查信息，没有头像、首字母圆圈或虚构形象。
- 公共窄图标导航、A/B 背景和 5/7/9 密度切换落到 React 代码。首次背景 B，7 是可随时修改的密度起点；两者分别持久化。
- 大量封面按行向下排列；窄窗减列，全选覆盖当前完整筛选中的可下载作品，过滤已入库、已排队和待复核项。一次确认后的队列行为继续受现有演示状态机约束。
- 外观配置允许选择本机 PNG/JPEG/WebP。当前浏览器阶段上限为 2 MiB，并限制图片尺寸；需要实际解码，读取失败保留前图。图片只存当前浏览器，不随代码上传。
- 设置按页保存草稿；存储失败保留草稿，恢复默认背景不修改密度。下载资源只保留同时下载作品和全局图片请求数，不假装它们已经控制真实下载器。
- 普通 UI 测试不再输出成功页面的预览截图；GitHub Actions 执行状态机/偏好测试、格式检查、构建及 Chromium 行为/布局验收，失败诊断单独保留。

## ZIP 与图像处理的固定源码核查

主 Agent 重新读取以下固定版本源码，并复核只读子 Agent 的调查结果。本次没有修改真实下载执行器、导出器或媒体材料。

| 来源 | 实际行为 | 证据 |
| --- | --- | --- |
| JM `f0cdd724...` | 非 GIF 图片解码后，按 `block_num` 恢复横向条带顺序，再编码保存；GIF 直接保存。条带还原有实际用途，不能当作无意义美化删除 | [download_img_task.rs 273–358](https://github.com/lanyeeee/jmcomic-downloader/blob/f0cdd724af6892002f2fb7be883b88832cebe7e9/src-tauri/src/downloader/download_img_task.rs#L273-L358) |
| JM 同版本 | 使用已下载章节目录中的图片，经本地 `ZipWriter` 创建章节 CBZ，附 `ComicInfo.xml`；不是服务器返回一作一个 ZIP | [cbz.rs 166–229](https://github.com/lanyeeee/jmcomic-downloader/blob/f0cdd724af6892002f2fb7be883b88832cebe7e9/src-tauri/src/export/cbz.rs#L166-L229) |
| 哔咔 `77c8b62...` | 逐张取得图片；源格式与目标相同时直接写原字节，否则转换格式。不能把所有图片都说成必须转码 | [download_manager.rs 754–825](https://github.com/lanyeeee/picacomic-downloader/blob/77c8b62ede42b3afc074506d092313816af8092d/src-tauri/src/download_manager.rs#L754-L825)、[916–958](https://github.com/lanyeeee/picacomic-downloader/blob/77c8b62ede42b3afc074506d092313816af8092d/src-tauri/src/download_manager.rs#L916-L958) |
| 哔咔同版本 | 本地读取章节图片，以 `ZipWriter` 创建章节 CBZ | [export.rs 103–173](https://github.com/lanyeeee/picacomic-downloader/blob/77c8b62ede42b3afc074506d092313816af8092d/src-tauri/src/export.rs#L103-L173) |

因此移除两个调节项，不移除必要的源图片还原与最终压缩包验证。我们的“一作一个 ZIP”由后台一次完成；若后续来源/适配器已提供符合约定的完整 ZIP，则先验证该成品，不重复拆包重打。这是实施约定，尚不是本轮已经交付的真实归档链。没有因此继承上游的覆盖、跳过、完成或删除规则。

## 尚未完成

当前数据仍是明确标注的虚构浏览器数据，在线收藏入口是按来源浏览的展示基础，不是已登录账户的真实收藏。漫画库库存/书单、作品关注、复核和排行等页面还需完整接线。账号与收藏操作、Tauri 桌面壳、安全凭据、真实队列与资源调度、ZIP/CBZ 库读取、可选 PDF/CBZ 导出、保存恢复及云端结果发布均需后续实现和独立验收。

本批没有真实下载、账号操作、媒体写入、生产启用或完整 V1 完成声明。下一步按已确认功能接入 JM 账号与收藏，再验收哔咔；原生命令和敏感数据不得由界面模拟状态制造权限。

## 验证记录

实现及独立审查进行中；最终提交和 GitHub Actions 结果在检查完成后补入。
