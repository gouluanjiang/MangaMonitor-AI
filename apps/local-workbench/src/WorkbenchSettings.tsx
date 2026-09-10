import { useEffect, useRef, useState } from "react";
import type { ChangeEvent, ReactNode } from "react";
import {
  initialPreferences,
  readBackgroundFile,
  resourcePreset,
  restoreDefaultBackground,
} from "./preferences.ts";
import type {
  BackgroundSelection,
  AppearancePreferences,
  CoverDensity,
  ResourcePreferences,
  WorkbenchPreferences,
} from "./preferences.ts";
import "./settings.css";

type SettingsPage =
  "accounts" | "library" | "appearance" | "resources" | "network";
const pages: { id: SettingsPage; label: string }[] = [
  { id: "accounts", label: "账号与收藏" },
  { id: "library", label: "漫画库" },
  { id: "appearance", label: "外观" },
  { id: "resources", label: "下载与资源" },
  { id: "network", label: "网络与诊断" },
];

function sameAppearance(a: AppearancePreferences, b: AppearancePreferences) {
  return (
    a.backgroundMode === b.backgroundMode &&
    a.density === b.density &&
    a.backgroundImage === b.backgroundImage &&
    a.backgroundName === b.backgroundName
  );
}

function sameResources(a: ResourcePreferences, b: ResourcePreferences) {
  return (
    a.profile === b.profile &&
    a.simultaneousWorks === b.simultaneousWorks &&
    a.imageRequests === b.imageRequests
  );
}

export interface WorkbenchSettingsProps {
  accountPanel?: ReactNode;
  libraryPanel?: ReactNode;
  preferences: WorkbenchPreferences;
  onSave(next: WorkbenchPreferences): Promise<boolean>;
  storageLabel: string;
  saveDisabled: boolean;
  onNativeChooseBackground?: () => Promise<BackgroundSelection | null>;
  onPreview(appearance: AppearancePreferences | null): void;
  onResetDemo(): void;
  storageFailed: boolean;
  searchQuery: string;
  onBackgroundValidated(dataUrl: string): void;
}

export function WorkbenchSettings({
  accountPanel,
  libraryPanel,
  preferences,
  onSave,
  storageLabel,
  saveDisabled,
  onNativeChooseBackground,
  onPreview,
  onResetDemo,
  storageFailed,
  searchQuery,
  onBackgroundValidated,
}: WorkbenchSettingsProps) {
  const [page, setPage] = useState<SettingsPage>("accounts");
  const [appearance, setAppearance] = useState(preferences.appearance);
  const [resources, setResources] = useState(preferences.resources);
  const [readingImage, setReadingImage] = useState(false);
  const [saving, setSaving] = useState(false);
  const savingRef = useRef(false);
  const [imageError, setImageError] = useState<string | null>(null);
  const [feedback, setFeedback] = useState<string | null>(null);
  const [saveError, setSaveError] = useState(false);
  const fileInput = useRef<HTMLInputElement>(null);
  const imageRequest = useRef(0);
  const previousPreferences = useRef(preferences);
  const previewCallback = useRef(onPreview);
  previewCallback.current = onPreview;

  useEffect(() => {
    const previous = previousPreferences.current;
    setAppearance((draft) =>
      sameAppearance(draft, previous.appearance)
        ? preferences.appearance
        : draft,
    );
    setResources((draft) =>
      sameResources(draft, previous.resources) ? preferences.resources : draft,
    );
    previousPreferences.current = preferences;
  }, [preferences]);

  useEffect(() => {
    previewCallback.current(page === "appearance" ? appearance : null);
  }, [page, appearance]);

  useEffect(
    () => () => {
      imageRequest.current += 1;
      previewCallback.current(null);
    },
    [],
  );

  const appearanceDirty = !sameAppearance(appearance, preferences.appearance);
  const resourcesDirty = !sameResources(resources, preferences.resources);
  const currentDirty = page === "appearance" ? appearanceDirty : resourcesDirty;
  const editablePage = page === "appearance" || page === "resources";
  const defaults = initialPreferences();
  const canRestorePage =
    page === "appearance"
      ? !sameAppearance(appearance, defaults.appearance) || readingImage
      : page === "resources" && !sameResources(resources, defaults.resources);

  function clearFeedback() {
    setFeedback(null);
    setSaveError(false);
  }

  function cancelImageRead() {
    imageRequest.current += 1;
    setReadingImage(false);
    setImageError(null);
    if (fileInput.current) fileInput.current.value = "";
  }

  async function chooseBackground(event: ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    if (!file) return;
    const request = ++imageRequest.current;
    event.target.value = "";
    setReadingImage(true);
    setImageError(null);
    clearFeedback();
    try {
      const selected = await readBackgroundFile(file);
      if (request !== imageRequest.current) return;
      setAppearance((draft) => ({ ...draft, ...selected }));
      onBackgroundValidated(selected.backgroundImage);
    } catch (error) {
      if (request !== imageRequest.current) return;
      setImageError(
        error instanceof Error ? error.message : "图片读取失败，原背景已保留。",
      );
    } finally {
      if (request === imageRequest.current) setReadingImage(false);
    }
  }

  async function chooseNativeBackground() {
    if (!onNativeChooseBackground || readingImage || savingRef.current) return;
    const request = ++imageRequest.current;
    setReadingImage(true);
    setImageError(null);
    clearFeedback();
    try {
      const selected = await onNativeChooseBackground();
      if (request !== imageRequest.current || selected === null) return;
      setAppearance((draft) => ({ ...draft, ...selected }));
      onBackgroundValidated(selected.backgroundImage);
    } catch {
      if (request === imageRequest.current)
        setImageError(
          "图片无法读取或解码，原背景已保留。请选择不超过 8 MiB 的 PNG、JPEG 或 WebP 图片。",
        );
    } finally {
      if (request === imageRequest.current) setReadingImage(false);
    }
  }

  function restorePage() {
    clearFeedback();
    if (page === "appearance") {
      cancelImageRead();
      setAppearance(defaults.appearance);
    } else if (page === "resources") {
      setResources(defaults.resources);
    }
    setFeedback("已恢复本页默认草稿，保存后记住这些设置。");
  }

  async function savePage() {
    if (
      !editablePage ||
      savingRef.current ||
      (page === "appearance" && readingImage)
    )
      return;
    savingRef.current = true;
    setSaving(true);
    const next: WorkbenchPreferences = {
      ...preferences,
      ...(page === "appearance" ? { appearance } : { resources }),
    };
    let saved = false;
    try {
      saved = await onSave(next);
    } catch {
      // The form keeps its draft if browser storage or a host callback fails.
    }
    savingRef.current = false;
    setSaving(false);
    setSaveError(!saved);
    setFeedback(
      saved
        ? page === "resources"
          ? `已保存到${storageLabel}。资源限制尚未连接真实调度器。`
          : `外观已保存到${storageLabel}。`
        : "保存失败，草稿已保留。请检查本机存储或重新读取后重试。",
    );
  }

  return (
    <div className="workbench-settings" data-testid="workbench-settings">
      <div className="page-heading">
        <div>
          <div className="eyebrow">YOUR WORKBENCH</div>
          <h1>设置</h1>
          <p>按自己的习惯整理漫画、背景和下载偏好。</p>
        </div>
      </div>
      <div className="settings-layout">
        <nav className="settings-navigation" aria-label="设置分类">
          {pages
            .filter((item) => item.label.includes(searchQuery.trim()))
            .map((item) => {
              const dirty =
                (item.id === "appearance" && appearanceDirty) ||
                (item.id === "resources" && resourcesDirty);
              return (
                <button
                  key={item.id}
                  type="button"
                  disabled={saving}
                  aria-label={item.label}
                  aria-current={page === item.id ? "page" : undefined}
                  data-testid={`settings-${item.id}`}
                  onClick={() => {
                    setPage(item.id);
                    clearFeedback();
                  }}
                >
                  {item.label}
                  {dirty && (
                    <span aria-hidden="true" title="有未保存的修改">
                      ·
                    </span>
                  )}
                </button>
              );
            })}
        </nav>
        <fieldset
          className="settings-panel"
          disabled={saving}
          aria-busy={saving}
        >
          {searchQuery.trim() &&
            !pages.some((item) => item.label.includes(searchQuery.trim())) && (
              <p role="status" className="settings-notice">
                没有匹配的设置分类，请试试账号、漫画库、外观、下载或网络。
              </p>
            )}
          {storageFailed && (
            <p className="settings-notice warning" role="status">
              本机存储当前不可用。修改会保留在本页草稿中，保存成功后才能在重开时恢复。
            </p>
          )}
          {page === "accounts" &&
            (accountPanel ?? (
              <section
                className="settings-card"
                aria-labelledby="accounts-title"
              >
                <h2 id="accounts-title">账号与收藏</h2>
                <p className="settings-copy">
                  JM
                  与哔咔分别连接账号。收藏更新汇总后，批量确认一次即可加入下载队列。
                </p>
                <div className="settings-account-list">
                  {["JM", "哔咔"].map((name) => (
                    <div className="settings-account-row" key={name}>
                      <div>
                        <strong>{name}</strong>
                        <p>账号登录与收藏同步待接入</p>
                      </div>
                      <span className="settings-pending">未连接</span>
                    </div>
                  ))}
                </div>
                <p className="settings-help">
                  当前页面不会收集账号或密码。在线收藏中的内容仍为演示数据。
                </p>
              </section>
            ))}
          {page === "library" &&
            (libraryPanel ?? (
              <section
                className="settings-card"
                aria-labelledby="library-title"
              >
                <h2 id="library-title">漫画库</h2>
                <p className="settings-copy">
                  每部作品保存为一个 ZIP，包内按章节分目录。现有 ZIP／CBZ
                  保持原格式。
                </p>
                <dl className="settings-facts">
                  <div>
                    <dt>保存目录</dt>
                    <dd>待接入本地目录选择</dd>
                  </div>
                  <div>
                    <dt>目录结构</dt>
                    <dd>漫画库／作品名.zip</dd>
                  </div>
                  <div>
                    <dt>同一作品、同一版本</dt>
                    <dd>默认保留一个来源，可手动选择另一来源</dd>
                  </div>
                  <div>
                    <dt>阅读</dt>
                    <dd>首版提供详情与下载管理，暂不内置阅读器</dd>
                  </div>
                </dl>
                <p className="settings-help">
                  本地库扫描与文件导入尚未接入，当前没有读取或修改电脑上的漫画文件。
                </p>
              </section>
            ))}
          {page === "appearance" && (
            <section
              className="settings-card"
              aria-labelledby="appearance-title"
            >
              <h2 id="appearance-title">背景与封面</h2>
              <p className="settings-copy">
                修改会在当前页面即时预览。保存后记住选择，离开设置时使用已保存的外观。
              </p>
              <div className="settings-background-picker">
                <div className="settings-background-name">
                  <h3>自定义背景</h3>
                  <span
                    className="settings-background-thumb"
                    aria-hidden="true"
                    style={
                      appearance.backgroundImage
                        ? {
                            backgroundImage: `url("${appearance.backgroundImage}")`,
                          }
                        : undefined
                    }
                  />
                  <p data-testid="background-filename">
                    {appearance.backgroundName ?? "使用默认背景"}
                  </p>
                </div>
                <div className="settings-inline-actions">
                  {onNativeChooseBackground ? (
                    <button
                      type="button"
                      className="button secondary"
                      data-testid="native-background-file"
                      onClick={chooseNativeBackground}
                      disabled={readingImage}
                    >
                      选择背景图片
                    </button>
                  ) : (
                    <label className="button secondary settings-file-picker">
                      选择背景图片
                      <input
                        ref={fileInput}
                        type="file"
                        accept="image/png,image/jpeg,image/webp"
                        aria-label="选择背景图片"
                        data-testid="background-file"
                        onChange={chooseBackground}
                      />
                    </label>
                  )}
                  <button
                    type="button"
                    className="text-button"
                    data-testid="restore-default-background"
                    disabled={
                      appearance.backgroundImage === null && !readingImage
                    }
                    onClick={() => {
                      cancelImageRead();
                      setAppearance((draft) => restoreDefaultBackground(draft));
                      clearFeedback();
                    }}
                  >
                    恢复默认背景
                  </button>
                </div>
              </div>
              <p className="settings-help">
                支持 PNG、JPEG、WebP，最大{" "}
                {onNativeChooseBackground ? "8" : "2"} MiB、单边 8192 像素、总计
                2400 万像素。图片仅保存在{storageLabel}，不会上传。
                {onNativeChooseBackground &&
                  "保存后使用本机缓存，原图片移动不影响已保存背景。"}
              </p>
              {readingImage && (
                <p className="settings-help" role="status">
                  正在检查图片…
                </p>
              )}
              {imageError && (
                <p className="settings-notice warning" role="alert">
                  {imageError}
                </p>
              )}
              <h3 className="settings-field-title">背景模式</h3>
              <div
                className="settings-mode-options"
                role="group"
                aria-label="背景模式"
              >
                {(["A", "B"] as const).map((mode) => (
                  <button
                    type="button"
                    key={mode}
                    className={`settings-mode-option ${appearance.backgroundMode === mode ? "selected" : ""}`}
                    aria-pressed={appearance.backgroundMode === mode}
                    data-testid={`background-mode-${mode}`}
                    onClick={() => {
                      setAppearance((draft) => ({
                        ...draft,
                        backgroundMode: mode,
                      }));
                      clearFeedback();
                    }}
                  >
                    <span
                      className={`settings-mode-example mode-${mode}`}
                      style={
                        appearance.backgroundImage
                          ? {
                              backgroundImage: `url("${appearance.backgroundImage}")`,
                            }
                          : undefined
                      }
                      aria-hidden="true"
                    >
                      <span className="settings-mode-rail" />
                      <span className="settings-mode-grid">
                        {Array.from({ length: 6 }, (_, index) => (
                          <i key={index} />
                        ))}
                      </span>
                    </span>
                    <span className="settings-option-name">
                      {mode} · {mode === "A" ? "全窗暗背景" : "顶部渐隐"}
                    </span>
                    <span className="settings-option-description">
                      {mode === "A"
                        ? "背景铺满窗口，深色遮罩保证清晰"
                        : "背景集中在顶部，向下融入页面"}
                    </span>
                  </button>
                ))}
              </div>
              <div className="settings-density-row">
                <div>
                  <h3>封面密度</h3>
                  <p className="settings-help">
                    按行排列、向下滚动；窗口缩小时自动减少列数。
                  </p>
                </div>
                <div
                  className="settings-segmented"
                  role="group"
                  aria-label="封面密度"
                >
                  {([5, 7, 9] as CoverDensity[]).map((density) => (
                    <button
                      key={density}
                      type="button"
                      aria-pressed={appearance.density === density}
                      data-testid={`settings-density-${density}`}
                      onClick={() => {
                        setAppearance((draft) => ({ ...draft, density }));
                        clearFeedback();
                      }}
                    >
                      {density}{" "}
                      {density === 5
                        ? "大封面"
                        : density === 7
                          ? "标准"
                          : "紧凑"}
                    </button>
                  ))}
                </div>
              </div>
            </section>
          )}
          {page === "resources" && (
            <section
              className="settings-card"
              aria-labelledby="resources-title"
            >
              <h2 id="resources-title">下载资源</h2>
              <p className="settings-copy">
                控制同时处理的作品与图片请求，减少对电脑和网络的占用。
              </p>
              <div
                className="settings-segmented settings-profile-options"
                role="group"
                aria-label="资源模式"
              >
                {[
                  ["economy", "省资源"],
                  ["balanced", "均衡"],
                  ["custom", "自定义"],
                ].map(([profile, label]) => (
                  <button
                    key={profile}
                    type="button"
                    aria-pressed={resources.profile === profile}
                    data-testid={`resource-profile-${profile}`}
                    onClick={() => {
                      setResources((draft) =>
                        profile === "custom"
                          ? { ...draft, profile: "custom" }
                          : resourcePreset(profile as "economy" | "balanced"),
                      );
                      clearFeedback();
                    }}
                  >
                    <strong>{label}</strong>
                    <span className="resource-description">
                      {profile === "economy"
                        ? "较小的网络与磁盘占用"
                        : profile === "balanced"
                          ? "平衡速度与资源占用"
                          : "手动调整各项上限"}
                    </span>
                  </button>
                ))}
              </div>
              <p className="settings-notice warning">
                建议起点，尚未实测；可按电脑负载调整。
              </p>
              <div className="settings-resource-fields">
                <label>
                  <span>同时下载作品</span>
                  <select
                    aria-label="同时下载作品"
                    value={resources.simultaneousWorks}
                    disabled={resources.profile !== "custom"}
                    onChange={(event) => {
                      setResources((draft) => ({
                        ...draft,
                        simultaneousWorks: Number(event.target.value),
                      }));
                      clearFeedback();
                    }}
                  >
                    {[1, 2, 3, 4].map((count) => (
                      <option key={count} value={count}>
                        {count} 部
                      </option>
                    ))}
                  </select>
                  <small>同一时间处于下载过程中的作品数量</small>
                </label>
                <label>
                  <span>全局图片请求数</span>
                  <select
                    aria-label="全局图片请求数"
                    value={resources.imageRequests}
                    disabled={resources.profile !== "custom"}
                    onChange={(event) => {
                      setResources((draft) => ({
                        ...draft,
                        imageRequests: Number(event.target.value),
                      }));
                      clearFeedback();
                    }}
                  >
                    {[1, 2, 3, 4, 5, 6, 7, 8].map((count) => (
                      <option key={count} value={count}>
                        {count} 个
                      </option>
                    ))}
                  </select>
                  <small>所有作品合计的图片网络请求数量</small>
                </label>
              </div>
              <dl className="settings-facts">
                <div>
                  <dt>确认下载后</dt>
                  <dd>加入队列，有空闲名额时开始</dd>
                </div>
                <div>
                  <dt>关闭应用</dt>
                  <dd>保存进度、安全暂停并退出，下次打开恢复</dd>
                </div>
              </dl>
              <p className="settings-help">
                真实调度器尚未接入，目前只保存偏好。退出与恢复为桌面端约定，浏览器样例使用“模拟退出”演示。
              </p>
            </section>
          )}
          {page === "network" && (
            <>
              <section
                className="settings-card"
                aria-labelledby="network-title"
              >
                <h2 id="network-title">网络与诊断</h2>
                <p className="settings-copy">
                  来源连接检测、代理配置与诊断日志将在本地服务接入后提供。
                </p>
                <dl className="settings-facts">
                  <div>
                    <dt>JM／哔咔连接</dt>
                    <dd>待接入</dd>
                  </div>
                  <div>
                    <dt>本地下载器</dt>
                    <dd>尚未连接</dd>
                  </div>
                  <div>
                    <dt>诊断数据</dt>
                    <dd>当前没有采集网络或账号日志</dd>
                  </div>
                </dl>
              </section>
              <section className="settings-card" aria-labelledby="demo-title">
                <h2 id="demo-title">关于这个样例</h2>
                <p className="settings-copy">
                  作品、封面和队列状态均为演示内容。外观与资源偏好单独保存在
                  {storageLabel}，不会影响真实漫画库或线上账号。
                </p>
                <div className="settings-reset-row">
                  <div>
                    <h3>重新体验</h3>
                    <p className="settings-help">
                      重置模拟队列和页面选择，保留外观与资源偏好。
                    </p>
                  </div>
                  <button
                    type="button"
                    className="button secondary"
                    onClick={onResetDemo}
                  >
                    重置样例
                  </button>
                </div>
              </section>
            </>
          )}
          {editablePage && (
            <div className="settings-savebar">
              <div
                className={`settings-save-message ${saveError ? "warning" : ""}`}
                role="status"
              >
                {feedback ??
                  (currentDirty
                    ? "本页有未保存的修改"
                    : "本页设置已与保存内容一致")}
              </div>
              <div className="settings-inline-actions">
                <button
                  type="button"
                  className="button secondary"
                  disabled={!canRestorePage}
                  onClick={restorePage}
                  data-testid="restore-settings-page"
                >
                  还原本页
                </button>
                <button
                  type="button"
                  className="button primary"
                  disabled={
                    saveDisabled ||
                    !currentDirty ||
                    (page === "appearance" && readingImage)
                  }
                  onClick={savePage}
                  data-testid="save-settings-page"
                >
                  {saving ? "正在保存…" : "保存设置"}
                </button>
              </div>
            </div>
          )}
        </fieldset>
      </div>
    </div>
  );
}
