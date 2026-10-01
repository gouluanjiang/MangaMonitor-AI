import { useEffect, useRef, useState } from "react";
import { CompletionPanel } from "./CompletionPanel.tsx";
import type { CompletionPanelProps } from "./CompletionPanel.tsx";
import { authorQueryError, authorQueryMessage } from "./author-query.ts";
import "./author-workspace.css";

type Props = Omit<CompletionPanelProps, "adapter" | "searchTabId">;
interface Tab {
  id: string;
  name: string;
  requestKey: number;
}

/** Runtime-only query workspaces. Cross-source same-name reuse is user-selected;
 * this never merges author identities, attributions or source work IDs. */
export function AuthorWorkspace(props: Props) {
  const [tabs, setTabs] = useState<Tab[]>([]);
  const [selected, setSelected] = useState("");
  const [query, setQuery] = useState("");
  const [error, setError] = useState("");
  const serial = useRef(0);
  const handled = useRef<number | null>(null);
  const tabRef = useRef(tabs);
  tabRef.current = tabs;
  function open(raw: string) {
    const name = raw.trim();
    const issue = authorQueryError(name);
    if (!name || issue) {
      setError(authorQueryMessage(issue) ?? "请输入作者名。");
      return;
    }
    setError("");
    setQuery(name);
    const existing = tabRef.current.find((tab) => tab.name === name);
    if (existing) {
      setSelected(existing.id);
      return;
    }
    const requestKey = ++serial.current;
    const tab = { id: `author-tab-${requestKey}`, name, requestKey };
    tabRef.current = [...tabRef.current, tab];
    setTabs(tabRef.current);
    setSelected(tab.id);
  }
  useEffect(() => {
    const request = props.authorRequest;
    if (!props.active || !request || handled.current === request.key) return;
    handled.current = request.key;
    open(request.name);
  }, [props.active, props.authorRequest]);
  function close(id: string) {
    const index = tabRef.current.findIndex((tab) => tab.id === id);
    const remaining = tabRef.current.filter((tab) => tab.id !== id);
    tabRef.current = remaining;
    setTabs(remaining);
    if (selected === id)
      setSelected((remaining[index] ?? remaining[index - 1])?.id ?? "");
  }
  return (
    <>
      {props.active && (
        <section className="author-workspace-tools" aria-label="作者搜索标签">
          <form
            onSubmit={(event) => {
              event.preventDefault();
              open(query);
            }}
          >
            <label>
              作者名{" "}
              <input
                aria-label="搜索作者名"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
              />
            </label>
            <button
              className="primary-button"
              type="submit"
              data-testid="completion-start"
              disabled={!query.trim()}
            >
              搜索两站作品
            </button>
          </form>
          {error && <p role="alert">{error}</p>}
          <div className="author-tabs" role="tablist" aria-label="已打开的作者">
            {tabs.map((tab) => (
              <div className="author-tab" key={tab.id}>
                <button
                  role="tab"
                  id={tab.id}
                  aria-controls={`${tab.id}-panel`}
                  aria-selected={selected === tab.id}
                  tabIndex={selected === tab.id ? 0 : -1}
                  onKeyDown={(event) => {
                    if (
                      !["ArrowLeft", "ArrowRight", "Home", "End"].includes(
                        event.key,
                      )
                    )
                      return;
                    event.preventDefault();
                    const index = tabs.findIndex(
                      (value) => value.id === tab.id,
                    );
                    const next =
                      tabs[
                        event.key === "Home"
                          ? 0
                          : event.key === "End"
                            ? tabs.length - 1
                            : (index +
                                (event.key === "ArrowLeft" ? -1 : 1) +
                                tabs.length) %
                              tabs.length
                      ];
                    setSelected(next.id);
                    setQuery(next.name);
                    document.getElementById(next.id)?.focus();
                  }}
                  onClick={() => {
                    setSelected(tab.id);
                    setQuery(tab.name);
                  }}
                  title={tab.name}
                >
                  {tab.name}
                </button>
                <button
                  className="author-tab-close"
                  aria-label={`关闭作者标签 ${tab.name}`}
                  onClick={() => close(tab.id)}
                >
                  ×
                </button>
              </div>
            ))}
          </div>
          {!tabs.length && (
            <p className="source-muted">
              打开作者后会保留各自的结果与浏览位置，直到关闭标签或退出程序。
            </p>
          )}
        </section>
      )}
      {tabs.map((tab) => (
        <div
          role="tabpanel"
          id={`${tab.id}-panel`}
          aria-labelledby={tab.id}
          key={tab.id}
          hidden={!props.active || selected !== tab.id}
        >
          <CompletionPanel
            {...props}
            mode="search"
            searchTabId={tab.id}
            active={!!props.active && selected === tab.id}
            authorRequest={{ name: tab.name, key: tab.requestKey }}
          />
        </div>
      ))}
    </>
  );
}
