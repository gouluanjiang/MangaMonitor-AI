export type SettingsPage =
  "accounts" | "library" | "appearance" | "resources" | "network";
const pages: { id: SettingsPage; label: string; keywords: string }[] = [
  {
    id: "accounts",
    label: "账号与收藏",
    keywords: "登录 连接 退出 会话 记住 记住会话 JM Pica 哔咔 收藏",
  },
  {
    id: "library",
    label: "漫画库",
    keywords: "电脑 目录 文件 路径 ZIP 整理 映射 刷新 入库",
  },
  {
    id: "appearance",
    label: "外观",
    keywords: "背景 图片 壁纸 封面 密度 主题",
  },
  {
    id: "resources",
    label: "下载与资源",
    keywords: "下载 队列 速度 并发 资源 保存 ZIP",
  },
  {
    id: "network",
    label: "网络与诊断",
    keywords: "网络 诊断 状态 问题 版本 构建 反馈 摘要",
  },
];
export function matchingSettingsPages(query: string) {
  const terms = query
    .normalize("NFKC")
    .trim()
    .toLocaleLowerCase()
    .split(/\s+/)
    .filter(Boolean);
  return pages.filter((page) =>
    terms.every((term) =>
      (page.label + " " + page.keywords).toLocaleLowerCase().includes(term),
    ),
  );
}
