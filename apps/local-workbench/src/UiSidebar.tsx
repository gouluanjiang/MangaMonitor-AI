import type { ComponentProps, RefObject } from "react";
import { Icon } from "./icons.tsx";
import type { AccountSummary } from "./source-types.ts";

export type UiDestination =
  | "library"
  | "queue"
  | "favorites"
  | "recent"
  | "ranking"
  | "discovery"
  | "completion"
  | "author-search"
  | "authors"
  | "settings";
type Entry = {
  id: UiDestination;
  label: string;
  icon: ComponentProps<typeof Icon>["name"];
  native?: boolean;
};
const groups: { label: string; entries: Entry[] }[] = [
  {
    label: "我的漫画",
    entries: [
      { id: "library", label: "漫画库", icon: "library" },
      { id: "queue", label: "下载队列", icon: "download" },
    ],
  },
  {
    label: "发现作品",
    entries: [
      { id: "favorites", label: "在线收藏", icon: "heart" },
      { id: "recent", label: "最近更新", icon: "clock", native: true },
      { id: "ranking", label: "周排行榜", icon: "discover", native: true },
      { id: "discovery", label: "搜索", icon: "search" },
    ],
  },
  {
    label: "关注与更新",
    entries: [
      {
        id: "completion",
        label: "作者更新",
        icon: "completeness",
        native: true,
      },
      { id: "authors", label: "关注作者", icon: "people" },
    ],
  },
];

export function UiSidebar({
  sidebarRef,
  active,
  native,
  collapsed,
  accounts,
  unfinished,
  onNavigate,
  onToggle,
  onOpenAccounts,
}: {
  sidebarRef: RefObject<HTMLElement | null>;
  active: UiDestination;
  native: boolean;
  collapsed: boolean;
  accounts: AccountSummary[];
  unfinished: number;
  onNavigate(destination: UiDestination): void;
  onToggle(): void;
  onOpenAccounts(): void;
}) {
  const connected = accounts
    .filter((account) => account.state === "connected")
    .map((account) => (account.source === "Pica" ? "哔咔" : account.source));
  const connectionLabel = native
    ? connected.length
      ? connected.join(" · ") + " 已连接"
      : "来源尚未连接"
    : "交互样例";
  const toggleLabel = collapsed ? "展开侧栏" : "收起侧栏";
  const entry = ({ id, label, icon }: Entry) => (
    <button
      key={id}
      className={`nav-item ui-nav-item ${active === id ? "active" : ""}`}
      data-testid={`nav-${id}`}
      title={label}
      aria-label={label}
      aria-current={active === id ? "page" : undefined}
      onClick={() => onNavigate(id)}
    >
      <Icon name={icon} size={19} />
      <span className="ui-nav-label">{label}</span>
      {id === "queue" && unfinished > 0 && (
        <span className="nav-count ui-nav-count">{unfinished}</span>
      )}
    </button>
  );
  return (
    <aside
      ref={sidebarRef}
      className="sidebar ui-sidebar"
      aria-label="工作台侧栏"
    >
      <button
        className="brand"
        aria-label="MangaMonitor 首页"
        onClick={() => onNavigate("library")}
      >
        <span className="brand-mark">
          M<span />
        </span>
        <span className="ui-brand-name">
          MangaMonitor<small>你的漫画，井然有序。</small>
        </span>
      </button>
      <nav
        className="ui-navigation"
        aria-label="主要导航"
        id="workbench-navigation"
      >
        {groups.map((group) => (
          <section className="ui-nav-group" key={group.label}>
            <p className="ui-nav-heading">{group.label}</p>
            {group.entries.filter((item) => native || !item.native).map(entry)}
          </section>
        ))}
      </nav>
      <div className="sidebar-bottom">
        <button
          className="ui-connection"
          title={connectionLabel}
          aria-label={connectionLabel + "，打开账号设置"}
          onClick={onOpenAccounts}
        >
          <span
            className={`ui-connection-dot ${connected.length ? "connected" : ""}`}
          />
          <span className="ui-nav-label">{connectionLabel}</span>
        </button>
        {entry({ id: "settings", label: "设置", icon: "settings" })}
        <button
          className="nav-item ui-nav-item ui-sidebar-toggle"
          data-testid="sidebar-toggle"
          aria-label={toggleLabel}
          title={toggleLabel}
          aria-expanded={!collapsed}
          aria-controls="workbench-navigation"
          onClick={onToggle}
        >
          <Icon name={collapsed ? "arrow" : "back"} size={19} />
          <span className="ui-nav-label">{toggleLabel}</span>
        </button>
      </div>
    </aside>
  );
}
