import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import {
  addBooklistMembers,
  archiveBooklist,
  createBooklist,
  renameBooklist,
  restoreBooklist,
} from "./booklists.ts";
import type { BooklistsDocument, WorkReference } from "./booklists.ts";
import "./booklists.css";

export interface BooklistControlsProps {
  document: BooklistsDocument;
  selectedId: string | null;
  onSelect(id: string | null): void;
  onChange(next: BooklistsDocument): Promise<boolean>;
  onReload(): Promise<void>;
  disabled?: boolean;
  reloadDisabled?: boolean;
}

export interface BooklistPickerProps {
  document: BooklistsDocument;
  members: WorkReference[];
  onChange(next: BooklistsDocument): Promise<boolean>;
  onReload(): Promise<void>;
  onClose(): void;
  disabled?: boolean;
  reloadDisabled?: boolean;
}

function useBooklistWrite(
  onChange: (next: BooklistsDocument) => Promise<boolean>,
  onReload: () => Promise<void>,
  disabled: boolean,
  reloadDisabled: boolean,
) {
  const lock = useRef(false);
  const [pending, setPending] = useState(false);
  const [reloading, setReloading] = useState(false);
  const [error, setError] = useState("");
  async function commit(
    operation: () => BooklistsDocument,
    success: () => void,
  ) {
    if (disabled || lock.current) return;
    setError("");
    let next: BooklistsDocument;
    try {
      next = operation();
    } catch (cause) {
      setError(
        cause instanceof Error ? cause.message : "无法修改书单，请检查输入。",
      );
      return;
    }
    lock.current = true;
    setPending(true);
    try {
      const saved = await onChange(next);
      if (saved === true) {
        success();
      } else {
        setError("保存失败，草稿和选择已保留，请重试。");
      }
    } catch {
      setError("保存失败，草稿和选择已保留，请重试。");
    } finally {
      lock.current = false;
      setPending(false);
    }
  }
  async function reload() {
    if (reloadDisabled || lock.current) return;
    lock.current = true;
    setPending(true);
    setReloading(true);
    try {
      await onReload();
    } catch {
      setError("重新读取失败，草稿和选择已保留，请重试。");
    } finally {
      lock.current = false;
      setPending(false);
      setReloading(false);
    }
  }
  return { pending, reloading, error, setError, commit, reload };
}

function BooklistDialog({
  title,
  testId,
  pending,
  onClose,
  children,
}: {
  title: string;
  testId: string;
  pending: boolean;
  onClose(): void;
  children: ReactNode;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const dialog = ref.current;
    if (dialog && !dialog.open) dialog.showModal();
  }, []);
  return (
    <dialog
      ref={ref}
      className="dialog booklist-dialog"
      aria-label={title}
      aria-busy={pending}
      data-testid={testId}
      onCancel={(event) => {
        event.preventDefault();
        if (!pending) onClose();
      }}
    >
      <div className="dialog-heading">
        <h2>{title}</h2>
        <button
          type="button"
          className="text-button"
          aria-label="关闭书单窗口"
          disabled={pending}
          onClick={onClose}
        >
          关闭
        </button>
      </div>
      {children}
    </dialog>
  );
}

export function BooklistControls({
  document,
  selectedId,
  onSelect,
  onChange,
  onReload,
  disabled = false,
  reloadDisabled = false,
}: BooklistControlsProps) {
  const [editor, setEditor] = useState<
    { kind: "create" } | { kind: "rename"; id: string } | null
  >(null);
  const [name, setName] = useState("");
  const draftId = useRef<string | null>(null);
  const { pending, reloading, error, setError, commit, reload } =
    useBooklistWrite(onChange, onReload, disabled, reloadDisabled);
  const active = document.lists.filter((list) => !list.archived);
  const archived = document.lists.filter((list) => list.archived);
  const selected = active.find((list) => list.id === selectedId);
  const blocked = pending || disabled;

  function startCreate() {
    setError("");
    setName("");
    draftId.current = null;
    setEditor({ kind: "create" });
  }
  function startRename(id: string) {
    const list = document.lists.find((item) => item.id === id);
    if (!list) return;
    setError("");
    setName(list.name);
    setEditor({ kind: "rename", id });
  }
  function closeEditor() {
    if (pending) return;
    setEditor(null);
    setError("");
  }
  async function saveEditor() {
    if (!editor) return;
    let createdId: string | null = null;
    await commit(
      () => {
        const now = Date.now();
        if (editor.kind === "rename") {
          return renameBooklist(document, editor.id, name, now);
        }
        draftId.current ??= crypto.randomUUID();
        createdId = draftId.current;
        return createBooklist(document, { id: createdId, name, now });
      },
      () => {
        setEditor(null);
        if (createdId) onSelect(createdId);
      },
    );
  }

  return (
    <section
      className="booklist-controls"
      aria-label="管理本地书单"
      aria-busy={pending}
      data-testid="booklist-controls"
    >
      <div className="booklist-toolbar">
        <label className="booklist-select">
          <span>当前书单</span>
          <select
            value={selected?.id ?? ""}
            onChange={(event) => onSelect(event.target.value || null)}
            disabled={blocked || !active.length}
            data-testid="booklist-select"
          >
            <option value="">选择书单</option>
            {active.map((list) => (
              <option key={list.id} value={list.id}>
                {list.name} · {list.members.length} 部
              </option>
            ))}
          </select>
        </label>
        <div className="booklist-actions">
          <button
            type="button"
            className="button secondary"
            onClick={startCreate}
            disabled={blocked}
            data-testid="booklist-create"
          >
            新建书单
          </button>
          {selected && (
            <>
              <button
                type="button"
                className="text-button"
                onClick={() => startRename(selected.id)}
                disabled={blocked}
                data-testid="booklist-rename"
              >
                改名
              </button>
              <button
                type="button"
                className="text-button"
                disabled={blocked}
                data-testid="booklist-archive"
                onClick={() => {
                  void commit(
                    () => archiveBooklist(document, selected.id, Date.now()),
                    () => onSelect(null),
                  );
                }}
              >
                归档书单
              </button>
            </>
          )}
        </div>
      </div>
      {!active.length ? (
        <div className="booklist-empty" data-testid="booklist-empty">
          <h3>还没有可用的本地书单</h3>
          <p>新建书单后，可从作品详情或多选列表加入 JM 与哔咔作品。</p>
        </div>
      ) : !selected ? (
        <p className="booklist-help">选择一个书单，查看和整理其中的作品。</p>
      ) : null}
      <p className="booklist-help">
        书单可以混合不同来源。归档会保留名称和全部作品关联，可随时恢复。
      </p>
      {pending && (
        <p className="booklist-help" role="status">
          {reloading ? "正在重新读取书单…" : "正在保存书单…"}
        </p>
      )}
      {error && !editor && (
        <p className="booklist-error" role="alert">
          {error}
        </p>
      )}
      {archived.length > 0 && (
        <details className="booklist-archived">
          <summary data-testid="booklist-archived-toggle">
            已归档书单（{archived.length}）
          </summary>
          <ul>
            {archived.map((list) => (
              <li key={list.id} data-testid={"archived-booklist-" + list.id}>
                <div className="booklist-archived-name">
                  <strong>{list.name}</strong>
                  <span>{list.members.length} 部作品</span>
                </div>
                <div className="booklist-actions">
                  <button
                    type="button"
                    className="text-button"
                    aria-label={"改名已归档书单 " + list.name}
                    onClick={() => startRename(list.id)}
                    disabled={blocked}
                    data-testid={"booklist-rename-" + list.id}
                  >
                    改名
                  </button>
                  <button
                    type="button"
                    className="button secondary"
                    aria-label={"恢复书单 " + list.name}
                    disabled={blocked}
                    data-testid={"booklist-restore-" + list.id}
                    onClick={() => {
                      void commit(
                        () => restoreBooklist(document, list.id, Date.now()),
                        () => onSelect(list.id),
                      );
                    }}
                  >
                    恢复
                  </button>
                </div>
              </li>
            ))}
          </ul>
        </details>
      )}
      {editor && (
        <BooklistDialog
          title={editor.kind === "create" ? "新建书单" : "书单改名"}
          testId="booklist-editor"
          pending={pending}
          onClose={closeEditor}
        >
          <form
            onSubmit={(event) => {
              event.preventDefault();
              void saveEditor();
            }}
          >
            <label className="booklist-name-field">
              <span>书单名称</span>
              <input
                autoFocus
                type="text"
                value={name}
                onChange={(event) => {
                  setName(event.target.value);
                  setError("");
                }}
                disabled={blocked}
                data-testid="booklist-name"
                autoComplete="off"
                aria-describedby="booklist-name-help"
              />
            </label>
            <p id="booklist-name-help" className="booklist-help">
              最多 80 个字符，名称不能与其他未归档书单重复。
            </p>
            {error && (
              <div>
                <p className="booklist-error" role="alert">
                  {error}
                </p>
                <button
                  type="button"
                  className="text-button"
                  disabled={pending || reloadDisabled}
                  onClick={() => void reload()}
                  data-testid="booklist-editor-reload"
                >
                  {reloading ? "正在读取…" : "重新读取书单"}
                </button>
                <p className="booklist-help">
                  重新读取会保留草稿，请核对最新书单后再次保存。
                </p>
              </div>
            )}
            <div className="dialog-actions">
              <button
                type="button"
                className="button secondary"
                disabled={pending}
                onClick={closeEditor}
              >
                取消
              </button>
              <button
                type="submit"
                className="button primary"
                disabled={blocked || !name.trim()}
                data-testid="booklist-save"
              >
                {pending ? (reloading ? "正在读取…" : "正在保存…") : "保存书单"}
              </button>
            </div>
          </form>
        </BooklistDialog>
      )}
    </section>
  );
}

export function BooklistPicker({
  document,
  members,
  onChange,
  onReload,
  onClose,
  disabled = false,
  reloadDisabled = false,
}: BooklistPickerProps) {
  const active = document.lists.filter((list) => !list.archived);
  const [mode, setMode] = useState<"existing" | "create">(
    active.length ? "existing" : "create",
  );
  const [selected, setSelected] = useState<string[]>([]);
  const [name, setName] = useState("");
  const draftId = useRef<string | null>(null);
  const { pending, reloading, error, setError, commit, reload } =
    useBooklistWrite(onChange, onReload, disabled, reloadDisabled);
  const blocked = pending || disabled;
  const memberKeys = new Set(
    members.map((member) => member.source + ":" + member.workId),
  );

  async function addMembers() {
    if (!members.length || (mode === "existing" && !selected.length)) return;
    await commit(() => {
      const now = Date.now();
      if (mode === "create") {
        draftId.current ??= crypto.randomUUID();
        const next = createBooklist(document, {
          id: draftId.current,
          name,
          now,
        });
        return addBooklistMembers(next, draftId.current, members, now);
      }
      let next = document;
      for (const id of selected) {
        next = addBooklistMembers(next, id, members, now);
      }
      return next;
    }, onClose);
  }

  return (
    <BooklistDialog
      title="加入本地书单"
      testId="booklist-picker"
      pending={pending}
      onClose={onClose}
    >
      <p className="booklist-help">
        将 {memberKeys.size}{" "}
        部作品加入书单。可选择多个书单，已有作品不会重复加入。
      </p>
      <div
        className="booklist-picker-modes"
        role="group"
        aria-label="加入书单方式"
      >
        <button
          type="button"
          className="text-button"
          aria-pressed={mode === "existing"}
          disabled={blocked || !active.length}
          data-testid="booklist-picker-existing"
          onClick={() => {
            setMode("existing");
            setError("");
          }}
        >
          选择已有书单
        </button>
        <button
          type="button"
          className="text-button"
          aria-pressed={mode === "create"}
          disabled={blocked}
          data-testid="booklist-picker-create"
          onClick={() => {
            setMode("create");
            setError("");
          }}
        >
          新建并加入
        </button>
      </div>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void addMembers();
        }}
      >
        {mode === "existing" ? (
          <fieldset className="booklist-picker-options" disabled={blocked}>
            <legend className="sr-only">选择目标书单</legend>
            {active.map((list) => {
              const existing = new Set(
                list.members.map(
                  (member) => member.source + ":" + member.workId,
                ),
              );
              const alreadyIncluded =
                memberKeys.size > 0 &&
                [...memberKeys].every((key) => existing.has(key));
              return (
                <label key={list.id}>
                  <input
                    type="checkbox"
                    checked={selected.includes(list.id)}
                    disabled={blocked || alreadyIncluded}
                    data-testid={"booklist-target-" + list.id}
                    onChange={(event) => {
                      setSelected((previous) =>
                        event.target.checked
                          ? [...previous, list.id]
                          : previous.filter((id) => id !== list.id),
                      );
                      setError("");
                    }}
                  />
                  <span>
                    <strong>{list.name}</strong>
                    <small>
                      {alreadyIncluded
                        ? "已包含所选作品"
                        : list.members.length + " 部作品"}
                    </small>
                  </span>
                </label>
              );
            })}
            {!active.length && (
              <p className="booklist-help">暂无可加入的书单，请新建书单。</p>
            )}
          </fieldset>
        ) : (
          <label className="booklist-name-field">
            <span>新书单名称</span>
            <input
              type="text"
              value={name}
              onChange={(event) => {
                setName(event.target.value);
                setError("");
              }}
              disabled={blocked}
              data-testid="booklist-picker-name"
              autoComplete="off"
            />
          </label>
        )}
        {!members.length && (
          <p className="booklist-error" role="alert">
            尚未选择作品，请返回列表重新选择。
          </p>
        )}
        {error && (
          <div>
            <p className="booklist-error" role="alert">
              {error}
            </p>
            <button
              type="button"
              className="text-button"
              disabled={pending || reloadDisabled}
              onClick={() => void reload()}
              data-testid="booklist-picker-reload"
            >
              {reloading ? "正在读取…" : "重新读取书单"}
            </button>
            <p className="booklist-help">
              重新读取会保留草稿和选择，请核对最新书单后再次保存。
            </p>
          </div>
        )}
        <div className="dialog-actions">
          <button
            type="button"
            className="button secondary"
            disabled={pending}
            onClick={onClose}
          >
            取消
          </button>
          <button
            type="submit"
            className="button primary"
            data-testid="booklist-picker-save"
            disabled={
              blocked ||
              !members.length ||
              (mode === "create" ? !name.trim() : !selected.length)
            }
          >
            {pending
              ? reloading
                ? "正在读取…"
                : "正在保存…"
              : mode === "create"
                ? "创建并加入"
                : "加入所选书单"}
          </button>
        </div>
      </form>
    </BooklistDialog>
  );
}
