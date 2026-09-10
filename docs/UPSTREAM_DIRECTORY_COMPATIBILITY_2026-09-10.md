# JM/Pica 下载目录兼容与复用记录

用户要求未来 MangaMonitor 下载结果与其已安装的 lanyeeee JM/Pica 下载器保持相同目录格式，并优先参考开源实现。本次重新读取已有 pinned revisions，不改变协议 pins、不执行外部下载器，不修改真实下载或文件落盘执行链。

## 已对照的源码

- JM [`types/comic.rs`](https://github.com/lanyeeee/jmcomic-downloader/blob/f0cdd724af6892002f2fb7be883b88832cebe7e9/src-tauri/src/types/comic.rs)：`元数据.json`、`cover.jpg`、数字作品 ID、作者/标签数组、章节记录。
- JM [`types/chapter_info.rs`](https://github.com/lanyeeee/jmcomic-downloader/blob/f0cdd724af6892002f2fb7be883b88832cebe7e9/src-tauri/src/types/chapter_info.rs)：`章节元数据.json`、可配置目录命名和 `.下载中-章节名` 暂存目录。
- JM [`download_task.rs`](https://github.com/lanyeeee/jmcomic-downloader/blob/f0cdd724af6892002f2fb7be883b88832cebe7e9/src-tauri/src/downloader/download_task.rs) / [`download_img_task.rs`](https://github.com/lanyeeee/jmcomic-downloader/blob/f0cdd724af6892002f2fb7be883b88832cebe7e9/src-tauri/src/downloader/download_img_task.rs)：章节枚举、等待图片任务结束、图片还原和四位页码文件名。
- Pica [`types/comic.rs`](https://github.com/lanyeeee/picacomic-downloader/blob/77c8b62ede42b3afc074506d092313816af8092d/src-tauri/src/types/comic.rs) / [`download_manager.rs`](https://github.com/lanyeeee/picacomic-downloader/blob/77c8b62ede42b3afc074506d092313816af8092d/src-tauri/src/download_manager.rs)：24 位字符串 ID、标题/作者、章节及分页字段、`元数据.json`、`cover.扩展名`、章节图片与暂存目录。
- [`JMComic-Crawler-Python/jm_downloader.py`](https://github.com/hect0x7/JMComic-Crawler-Python/blob/9fddb0494caf0cdc812ac6cbfc1c62f4f845b058/src/jmcomic/jm_downloader.py)：任务聚合、图片/章节成功清单与整体完成条件，作为后续执行流程参考。

本批复用的是已确认的数据格式和组织约定：读取两种不同 JSON 形状、提取明确来源编号、优先作品封面、统计正式章节图片、忽略下载中的章节。元数据中的 URL/路径不授予网络或文件访问权。现有 MangaMonitor 来源模块已采用的协议参考继续沿用。

## 目标输出格式

```text
作品目录/
  元数据.json
  cover.jpg（Pica 可为其他已支持图像扩展名）
  章节目录/
    章节元数据.json
    0001.jpg
    0002.jpg
```

目录/图像具体命名按来源和已选格式适配；不引入 PDF/CBZ 导出选项。传到手机后电脑副本继续保留。当前电脑库名与手机 TXT 的精确核对不改变任何目录名字。

## 后续下载批次的接法

继续复用现有 JM/Pica 请求、枚举、图片还原与任务调度实现，补足界面到执行器及最终目录的连接。不能直接调用上游会清理临时目录、重命名目标或补写旧版章节元数据的读取函数，因为本批读库不应写原文件。未来落盘需遵守本项目已有 staging、无覆盖、明确完成和保留电脑副本的行为。

此记录不是完整的执行器解冻审计。真正修改下载/落盘执行代码前，仍按 `DOWNLOAD_EXECUTOR_THAW_GATE.md` 核对当时的源码、审批和完成条件。上游 MIT 许可与既有版权通知继续保留，复制实质代码时附对应声明。实际 EXE 的通用版本资源不足以证明其构建提交；本次格式适配依据 pinned 源码与本地目录样本，不宣称两者二进制完全相同。
