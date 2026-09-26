import { mkdir } from "node:fs/promises";
import { expect, test, type Page } from "@playwright/test";
import type { LibrarySnapshot } from "../src/library-types.ts";
// Legacy fixture is intentionally inaccessible to the product.
type PhoneLibrarySnapshot = {
  revision: number;
  importedNames: string[];
  importedAt: number | null;
  importFileName: string | null;
  manualEntries: any[];
};

// Synthetic Chromium/native-IPC interaction only. This suite never reads a real
// TXT, PC directory, phone, archive, credential, website or production inventory.
test.use({ storageState: { cookies: [], origins: [] } });
type Call = { command: string; args: Record<string, unknown> };
type Options = {
  failRevealOnce?: boolean;
  failPreferences?: boolean;
  pcCount?: number;
  phoneCount?: number;
  covers?: boolean;
  unicode?: boolean;
  selectCount?: number;
  cancelChoose?: boolean;
  holdNext?: boolean;
  failNextOnce?: boolean;
  cancelImportOnce?: boolean;
  failImportOnce?: boolean;
  failPhoneRead?: boolean;
  namespaceMarks?: boolean;
  importedNames?: string[];
  usability?: boolean;
  workDates?: boolean;
  languageTags?: string[][];
};
type Hooks = {
  copiedSummary?: string;
  calls: Call[];
  pc: LibrarySnapshot;
  phone: PhoneLibrarySnapshot;
  held: boolean;
  release?: () => void;
  coverActive: number;
  coverMax: number;
  phoneReadBlocked: boolean;
};
declare global {
  interface Window {
    libraryTest: Hooks;
  }
}
const id = (number: number) => number.toString(16).padStart(64, "0");
const errors = new WeakMap<Page, string[]>();
test.beforeEach(async ({ page }) => {
  const captured: string[] = [];
  errors.set(page, captured);
  page.on("pageerror", (error) => captured.push(error.message));
});
test.afterEach(async ({ page }) => {
  expect(errors.get(page) ?? [], "browser runtime errors").toEqual([]);
  const forbidden = await page.evaluate(() =>
    (window.libraryTest?.calls ?? []).filter(
      (call) =>
        !["jm_download_read", "download_inventory_read"].includes(
          call.command,
        ) &&
        /download|enqueue|delete|remove_file|move_file|source_set_favorite|source_favorite|production|promote/.test(
          call.command,
        ),
    ),
  );
  expect(forbidden).toEqual([]);
  const reads = await page.evaluate(() =>
    (window.libraryTest?.calls ?? []).filter(
      (call) => call.command === "jm_download_read",
    ),
  );
  expect(reads.map((call) => call.args)).toEqual(
    reads.map(() => ({ recheckFiles: true })),
  );
});

async function installMock(page: Page, options: Options = {}) {
  await page.addInitScript((options: Options) => {
    const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;
    const rootId = "a".repeat(64);
    const id = (number: number) => number.toString(16).padStart(64, "0");
    const item = (number: number): LibrarySnapshot["items"][number] => {
      const base =
        options.namespaceMarks && number <= 2
          ? "合成同名作品 [翻译甲]"
          : options.unicode && number <= 2
            ? number === 1
              ? "[合成作者] Cafe\u0301 A [翻译甲]"
              : "[合成作者] Café A [翻译乙]"
            : "合成电脑作品 " + String(number).padStart(4, "0");
      const reference: LibrarySnapshot["items"][number]["sourceRef"] =
        options.namespaceMarks && number <= 2
          ? number === 1
            ? { source: "JM", workId: "123" }
            : { source: "Pica", workId: "0123456789abcdef01234567" }
          : null;
      return {
        id: id(number),
        relativePath: reference ? reference.source + "/" + base : base,
        fileName: base,
        format: "directory",
        title: base,
        authors: ["合成作者"],
        description: null,
        tags: options.languageTags?.[number - 1] ?? [],
        bytes: 4096,
        modifiedAt: 1800000000000,
        addedAt:
          options.usability && number < 3
            ? 1800000000000 + number * 1000
            : null,
        versionUpdatedAt: options.workDates
          ? [null, "2026-09-02", "2026-09-01", "2026-09-02"][number - 1]
          : undefined,
        pageCount: 20,
        coverAvailable: Boolean(options.covers),
        state: options.usability && number === 3 ? "unreadable" : "indexed",
        errorCode:
          options.usability && number === 3 ? "LIBRARY_FILE_CHANGED" : null,
        sourceRef: reference,
        identityEvidence: reference ? "manual" : null,
      };
    };
    const savedPC = localStorage.getItem("synthetic.library.pc");
    const savedPhone = localStorage.getItem("synthetic.library.phone");
    const count = options.pcCount ?? 0;
    const initialPC: LibrarySnapshot = {
      revision: count ? 1 : 0,
      rootId: count ? rootId : null,
      rootPath: count ? "C:\\Synthetic PC Library" : null,
      generation: count ? 1 : 0,
      phase: count ? "complete" : "idle",
      freshness: count ? "cached" : "none",
      items: Array.from({ length: count }, (_, i) => item(i + 1)),
      visited: count,
      skipped: 0,
      updatedAt: count ? 1800000000000 : null,
      errorCode: null,
    };
    const initialPhone: PhoneLibrarySnapshot = {
      revision: options.namespaceMarks ? 1 : 0,
      importedNames: Array.from(
        { length: options.phoneCount ?? 0 },
        (_, i) => "合成手机作品 " + String(i + 1).padStart(4, "0") + ".zip",
      ),
      importedAt: options.phoneCount ? 1800000000000 : null,
      importFileName: options.phoneCount ? "合成手机名单.txt" : null,
      manualEntries: options.namespaceMarks
        ? [
            {
              id: id(900000),
              name: "合成同名作品 [翻译甲]",
              reference: { source: "JM", workId: "123" },
              markedAt: 1800000000000,
            },
          ]
        : [],
    };
    const hooks: Hooks = (window.libraryTest = {
      calls: [],
      pc: savedPC ? (JSON.parse(savedPC) as LibrarySnapshot) : initialPC,
      phone: savedPhone
        ? (JSON.parse(savedPhone) as PhoneLibrarySnapshot)
        : initialPhone,
      held: false,
      coverActive: 0,
      coverMax: 0,
      phoneReadBlocked: Boolean(options.failPhoneRead),
    });
    const savePC = () =>
      localStorage.setItem("synthetic.library.pc", JSON.stringify(hooks.pc));
    const savePhone = () =>
      localStorage.setItem(
        "synthetic.library.phone",
        JSON.stringify(hooks.phone),
      );
    let holdUsed = false;
    let failureUsed = false;
    let importCancelUsed = false;
    let importFailureUsed = false;
    const canvas = document.createElement("canvas");
    canvas.width = 2;
    canvas.height = 3;
    const context = canvas.getContext("2d")!;
    context.fillStyle = "#7959db";
    context.fillRect(0, 0, 2, 3);
    const thumbnail = canvas.toDataURL("image/jpeg");
    const preferences = {
      revision: 0,
      value: {
        version: 1,
        appearance: {
          backgroundMode: "B",
          density: 7,
          backgroundImage: null,
          backgroundName: null,
        },
        resources: {
          profile: "balanced",
          simultaneousWorks: 2,
          imageRequests: 4,
        },
      },
    };
    const booklists = { revision: 0, value: { version: 1, lists: [] } };
    let revealFailed = false;
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: {
        invoke: async (command: string, args: Record<string, unknown> = {}) => {
          hooks.calls.push({ command, args: clone(args) });
          if (command === "read_preferences" && options.failPreferences)
            throw { code: "STORAGE_UNAVAILABLE" };
          if (command === "workbench_info")
            return {
              version: "0.3.4",
              revision: "b".repeat(40),
              platform: "windows",
            };
          if (command === "library_reveal") {
            if (
              args.rootId !== hooks.pc.rootId ||
              args.generation !== hooks.pc.generation ||
              !hooks.pc.items.some((item) => item.id === args.entryId)
            )
              throw { code: "LIBRARY_STALE_SNAPSHOT" };
            if (options.failRevealOnce && !revealFailed) {
              revealFailed = true;
              throw { code: "LIBRARY_ENTRY_MISSING" };
            }
            return null;
          }
          if (command === "jm_download_read") return { revision: 0, tasks: [] };
          if (command === "source_matches_read")
            return { revision: 0, pairs: [] };
          if (command === "read_preferences") return clone(preferences);
          if (command === "read_booklists") return clone(booklists);
          if (command === "write_preferences") {
            preferences.revision++;
            preferences.value = clone(args.value) as typeof preferences.value;
            return clone(preferences);
          }
          if (command === "source_accounts")
            return ["JM", "Pica"].map((source) => ({
              source,
              sessionId: null,
              accountId: null,
              displayName: null,
              state: "disconnected",
              remembered: false,
              errorCode: null,
            }));
          if (command === "library_read")
            return clone({
              ...hooks.pc,
              freshness: hooks.pc.rootId ? "cached" : "none",
              phase: hooks.pc.phase === "reading" ? "paused" : hooks.pc.phase,
            });
          if (command === "library_import_paths") {
            if (
              args.rootId !== hooks.pc.rootId ||
              args.generation !== hooks.pc.generation
            )
              throw { code: "LIBRARY_STALE_GENERATION" };
            hooks.pc = {
              ...hooks.pc,
              revision: hooks.pc.revision + 1,
              phase: "paused",
            };
            savePC();
            return clone({
              snapshot: hooks.pc,
              mapped: 2,
              associated: 0,
              unchanged: 0,
            });
          }
          if (command === "phone_library_read") {
            if (hooks.phoneReadBlocked)
              throw { code: "PHONE_LIBRARY_UNAVAILABLE" };
            return clone(hooks.phone);
          }
          if (command === "library_choose") {
            if (options.cancelChoose) return null;
            hooks.pc = {
              ...hooks.pc,
              revision: hooks.pc.revision + 1,
              rootId,
              rootPath: "C:\\Synthetic PC Library",
              generation: hooks.pc.generation + 1,
              phase: "reading",
              freshness: "live",
              items: [],
              visited: 0,
              skipped: 0,
              updatedAt: 1800000000000,
              errorCode: null,
            };
            savePC();
            return clone(hooks.pc);
          }
          if (command === "library_scan") {
            if (
              args.rootId !== hooks.pc.rootId ||
              args.generation !== hooks.pc.generation
            )
              throw { code: "LIBRARY_STALE_GENERATION" };
            if (args.action === "pause")
              hooks.pc = { ...hooks.pc, phase: "paused" };
            else if (args.action === "start")
              hooks.pc = {
                ...hooks.pc,
                revision: hooks.pc.revision + 1,
                generation: hooks.pc.generation + 1,
                phase: "reading",
                items: [],
                visited: 0,
                errorCode: null,
              };
            else if (args.action === "resume")
              hooks.pc = { ...hooks.pc, phase: "reading", errorCode: null };
            else if (args.action === "next") {
              if (
                options.holdNext &&
                !holdUsed &&
                hooks.pc.items.length >= 20
              ) {
                holdUsed = true;
                hooks.held = true;
                await new Promise<void>((resolve) => {
                  hooks.release = () => {
                    hooks.held = false;
                    resolve();
                  };
                });
              }
              if (
                options.failNextOnce &&
                !failureUsed &&
                hooks.pc.items.length >= 20
              ) {
                failureUsed = true;
                hooks.pc = {
                  ...hooks.pc,
                  phase: "error",
                  errorCode: "LIBRARY_UNAVAILABLE",
                };
              } else {
                const total = options.selectCount ?? 60;
                const loaded = Math.min(total, hooks.pc.items.length + 20);
                hooks.pc = {
                  ...hooks.pc,
                  revision: hooks.pc.revision + 1,
                  items: Array.from({ length: loaded }, (_, i) => item(i + 1)),
                  visited: loaded,
                  phase: loaded === total ? "complete" : "reading",
                  errorCode: null,
                };
              }
            } else throw { code: "LIBRARY_INVALID_INPUT" };
            savePC();
            return clone(hooks.pc);
          }
          if (command === "library_cover") {
            if (
              args.rootId !== hooks.pc.rootId ||
              args.generation !== hooks.pc.generation ||
              !hooks.pc.items.some((entry) => entry.id === args.entryId)
            )
              throw { code: "LIBRARY_STALE_GENERATION" };
            hooks.coverActive++;
            hooks.coverMax = Math.max(hooks.coverMax, hooks.coverActive);
            await new Promise<void>((resolve) => setTimeout(resolve, 5));
            hooks.coverActive--;
            return {
              rootId: args.rootId,
              generation: args.generation,
              entryId: args.entryId,
              dataUrl: thumbnail,
            };
          }
          if (command === "library_link") {
            hooks.pc = {
              ...hooks.pc,
              revision: hooks.pc.revision + 1,
              items: hooks.pc.items.map((entry) =>
                entry.id === args.entryId
                  ? {
                      ...entry,
                      sourceRef: clone(
                        args.reference,
                      ) as typeof entry.sourceRef,
                      identityEvidence: args.reference ? "manual" : null,
                    }
                  : entry,
              ),
            };
            savePC();
            return clone(hooks.pc);
          }
          if (command.startsWith("phone_library_")) {
            if (args.revision !== hooks.phone.revision)
              throw { code: "REVISION_CONFLICT" };
            if (command === "phone_library_import") {
              if (options.cancelImportOnce && !importCancelUsed) {
                importCancelUsed = true;
                return null;
              }
              if (options.failImportOnce && !importFailureUsed) {
                importFailureUsed = true;
                throw { code: "PHONE_LIBRARY_INVALID" };
              }
              hooks.phone = {
                ...hooks.phone,
                revision: hooks.phone.revision + 1,
                importedNames: options.importedNames ?? [
                  "合成更新手机作品.zip",
                ],
                importedAt: 1800000000100,
                importFileName: "合成更新名单.txt",
              };
            } else if (command === "phone_library_mark") {
              const name = String(args.name).trim().normalize("NFC");
              const same = hooks.phone.manualEntries.find(
                (entry) =>
                  entry.name === name &&
                  JSON.stringify(entry.reference) ===
                    JSON.stringify(args.reference),
              );
              if (!same)
                hooks.phone = {
                  ...hooks.phone,
                  revision: hooks.phone.revision + 1,
                  manualEntries: [
                    ...hooks.phone.manualEntries,
                    {
                      id: id(900000 + hooks.phone.manualEntries.length),
                      name,
                      reference: clone(
                        args.reference,
                      ) as PhoneLibrarySnapshot["manualEntries"][number]["reference"],
                      markedAt: 1800000000200,
                    },
                  ],
                };
            } else if (command === "phone_library_unmark")
              hooks.phone = {
                ...hooks.phone,
                revision: hooks.phone.revision + 1,
                manualEntries: hooks.phone.manualEntries.filter(
                  (entry) => entry.id !== args.entryId,
                ),
              };
            else throw { code: "UNEXPECTED_SYNTHETIC_COMMAND" };
            savePhone();
            return clone(hooks.phone);
          }
          throw { code: "UNEXPECTED_SYNTHETIC_COMMAND" };
        },
      },
    });
  }, options);
  await page.goto("/");
  await expect(page.getByTestId("library-workbench")).toBeVisible();
}

test("library languages use saved version tags and leave linked historical versions unknown without source requests", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1672, height: 941 });
  await installMock(page, {
    pcCount: 4,
    namespaceMarks: true,
    languageTags: [[], ["日本語"], ["中文"], ["中文", "日本語"]],
  });
  for (const [index, label] of ["未知", "生肉", "已汉化", "未知"].entries()) {
    const badge = page
      .getByTestId("library-card-" + id(index + 1))
      .getByTestId("source-language-badge");
    await expect(badge).toHaveText(label);
    await expect(badge).toHaveAttribute("data-language-context", "local");
    await expect(badge).toHaveAttribute(
      "aria-label",
      new RegExp(`本地版本语言：${label}.*本地版本保存的标签`),
    );
  }
  // A known remote identity is not evidence of the saved package's language.
  expect(
    await page.evaluate(() => window.libraryTest.pc.items[0].sourceRef),
  ).toEqual({ source: "JM", workId: "123" });
  await page.getByTestId("library-open-" + id(1)).click();
  await page
    .getByTestId("reader-cover-actions")
    .getByRole("button", { name: "作品详情", exact: true })
    .click();
  await expect(page.getByTestId("library-detail")).toBeVisible();
  await page.getByTestId("library-detail-back").click();
  await expect(
    page
      .getByTestId("library-card-" + id(1))
      .getByTestId("source-language-badge"),
  ).toHaveText("未知");
  expect(
    await page.evaluate(() =>
      window.libraryTest.calls.filter((call) =>
        /^(source_query|source_cover|source_matches_)/.test(call.command),
      ),
    ),
  ).toEqual([]);
  await page.getByTestId("library-card-" + id(1)).scrollIntoViewIfNeeded();
  for (const number of [1, 2, 3, 4]) {
    await expect(
      page
        .getByTestId("library-card-" + id(number))
        .getByTestId("source-language-badge"),
    ).toBeInViewport({ ratio: 1 });
  }
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({
    path: "visual-evidence/language-local-versions.png",
  });
});

test("library detail reveals only the selected item and retains missing-file feedback for retry", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1672, height: 941 });
  await installMock(page, { pcCount: 3, failRevealOnce: true });
  await page.getByTestId("library-open-" + id(1)).click();
  await page
    .getByTestId("reader-cover-actions")
    .getByRole("button", { name: "作品详情", exact: true })
    .click();
  expect(await commands(page, "library_reveal")).toEqual([]);
  await page.getByTestId("library-reveal").click();
  await expect(page.getByTestId("library-location-status")).toContainText(
    "当前作品文件未找到",
  );
  await page.getByTestId("library-reveal").click();
  await expect(page.getByTestId("library-location-status")).toContainText(
    "文件资源管理器",
  );
  const calls = await commands(page, "library_reveal");
  expect(calls).toHaveLength(2);
  expect(calls[0].args).toEqual({
    rootId: "a".repeat(64),
    generation: 1,
    entryId: id(1),
  });
  expect(await commands(page, "library_scan")).toEqual([]);
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({ path: "visual-evidence/library-file-location.png" });
});

test("native diagnostics show real snapshot states, omit private data and link to the right settings", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1672, height: 941 });
  await installMock(page, { pcCount: 4, usability: true });
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("settings-network").click();
  await expect(page.getByTestId("diagnostics-version")).toContainText(
    "0.3.4 · bbbbbbb",
  );
  const report = await page.getByTestId("diagnostic-summary").inputValue();
  expect(report).toContain("目录记录：4 · 文件待核对：1");
  expect(report).toContain("JM 会话：未连接");
  expect(report).not.toMatch(
    /Synthetic PC Library|合成电脑作品|合成作者|sessionId/,
  );
  expect(await commands(page, "library_scan")).toEqual([]);
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({ path: "visual-evidence/native-diagnostics.png" });
  await page.evaluate(() => {
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        writeText: async (text: string) => {
          window.libraryTest.copiedSummary = text;
        },
      },
    });
  });
  await page.getByRole("button", { name: "复制诊断摘要", exact: true }).click();
  await expect(
    page.getByText("诊断摘要已复制。", { exact: true }),
  ).toBeVisible();
  expect(await page.evaluate(() => window.libraryTest.copiedSummary)).toBe(
    report,
  );
  await page.evaluate(() => {
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        writeText: async () => {
          throw new Error("synthetic clipboard unavailable");
        },
      },
    });
  });
  await page.getByRole("button", { name: "复制诊断摘要", exact: true }).click();
  await expect(
    page.getByText("无法自动复制，请复制下方已选中的文字。"),
  ).toBeVisible();
  await page.screenshot({
    path: "visual-evidence/native-diagnostics-copy.png",
  });
  await page.setViewportSize({ width: 900, height: 720 });
  expect(
    await page
      .locator("main")
      .evaluate((el) => el.scrollWidth <= el.clientWidth + 1),
  ).toBe(true);
  await page.setViewportSize({ width: 1672, height: 941 });
  await page.getByRole("button", { name: "漫画库设置", exact: true }).click();
  await expect(page.getByTestId("settings-library")).toHaveAttribute(
    "aria-current",
    "page",
  );
  await expect(page.getByTestId("library-import-paths")).toBeVisible();
  await page.getByTestId("settings-network").click();
  await page.getByRole("button", { name: "管理账号", exact: true }).click();
  await expect(page.getByTestId("source-account-settings")).toBeVisible();
  await page.getByTestId("settings-network").click();
  const count = (await commands(page, "source_accounts")).length;
  await page
    .getByRole("button", { name: "重新读取账号状态", exact: true })
    .click();
  await expect
    .poll(async () => (await commands(page, "source_accounts")).length)
    .toBe(count + 1);
  await page.getByRole("button", { name: "查看下载队列", exact: true }).click();
  await page.getByTestId("nav-settings").click();
  await expect(page.getByTestId("settings-network")).toHaveAttribute(
    "aria-current",
    "page",
  );
});

test("diagnostics stay reachable when preferences cannot be read, without allowing a default overwrite", async ({
  page,
}) => {
  await installMock(page, { pcCount: 3, failPreferences: true });
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("settings-network").click();
  await expect(page.getByTestId("diagnostic-summary")).toHaveValue(
    /设置：读取或保存有问题/,
  );
  await page.getByTestId("settings-appearance").click();
  await expect(
    page.getByText("外观设置尚未读入", { exact: false }),
  ).toBeVisible();
  await expect(page.getByTestId("save-settings-page")).toHaveCount(0);
  expect(await commands(page, "write_preferences")).toEqual([]);
});

test("settings keywords select matching sections and preserve appearance drafts while searching", async ({
  page,
}) => {
  await installMock(page, { pcCount: 3 });
  await page.getByTestId("nav-settings").click();
  const search = page.getByRole("textbox", { name: "搜索设置", exact: true });
  await search.fill("记住会话");
  await expect(page.getByTestId("settings-accounts")).toHaveAttribute(
    "aria-current",
    "page",
  );
  await search.fill("壁纸");
  await expect(page.getByTestId("settings-appearance")).toHaveAttribute(
    "aria-current",
    "page",
  );
  await page.getByTestId("settings-density-5").click();
  await search.fill("版本");
  await expect(page.getByTestId("diagnostics-panel")).toBeVisible();
  await search.fill("no-such-setting");
  await expect(
    page.getByText("没有找到相关设置", { exact: false }),
  ).toBeVisible();
  await expect(page.getByTestId("diagnostics-panel")).toHaveCount(0);
  await search.fill("");
  await page.getByTestId("settings-appearance").click();
  await expect(page.getByTestId("settings-density-5")).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await expect(page.getByTestId("save-settings-page")).toBeEnabled();
  expect(await commands(page, "write_preferences")).toEqual([]);
});

test("library admission sorting and state filters combine with search and preserve unknown dates", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1672, height: 941 });
  await installMock(page, { pcCount: 4, usability: true });
  const cards = page.getByTestId("library-grid").locator("article");
  await expect(cards.first()).toHaveAttribute("data-library-id", id(2));
  await page.getByTestId("library-sort").selectOption("added-asc");
  await expect(cards.first()).toHaveAttribute("data-library-id", id(1));
  await page.getByTestId("library-filter-review").click();
  await expect(cards).toHaveCount(1);
  await expect(cards.first()).toHaveAttribute("data-library-id", id(3));
  await page.getByTestId("library-filter-owned").click();
  await expect(cards).toHaveCount(3);
  await page.getByTestId("search-input").fill("0004");
  await expect(cards).toHaveCount(1);
  await page.getByTestId("library-open-" + id(4)).click();
  await page
    .getByTestId("reader-cover-actions")
    .getByRole("button", { name: "作品详情", exact: true })
    .click();
  await expect(page.getByTestId("library-added-at")).toContainText(
    "历史记录未知",
  );
  await page.getByTestId("library-detail-back").click();
  await page.getByTestId("search-input").fill("");
  await page.getByTestId("library-filter-all").click();
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({ path: "visual-evidence/library-usability.png" });
});
const commands = (page: Page, command: string) =>
  page.evaluate(
    (command) =>
      window.libraryTest.calls.filter((call) => call.command === command),
    command,
  );

test("library version and admission dates display independently, compose with filters and persist sorting", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1672, height: 1020 });
  await installMock(page, { pcCount: 4, usability: true, workDates: true });
  const cards = page.getByTestId("library-grid").locator("article");
  await page.getByTestId("library-sort").selectOption("updated-desc");
  await expect(cards).toHaveCount(4);
  await expect
    .poll(() =>
      cards.evaluateAll((nodes) =>
        nodes.map((node) => node.getAttribute("data-library-id")),
      ),
    )
    .toEqual([id(2), id(4), id(3), id(1)]);
  await expect(cards.first()).toContainText("版本更新：2026-09-02");
  await expect(cards.first()).toContainText("入库时间：");
  await page.getByTestId("library-sort").selectOption("updated-asc");
  await expect(cards.first()).toHaveAttribute("data-library-id", id(3));
  await page.getByTestId("library-filter-owned").click();
  await expect(cards).toHaveCount(3);
  await expect(cards.last()).toContainText("版本时间未知");
  await page.getByTestId("search-input").fill("0004");
  await expect(cards).toHaveCount(1);
  await page.getByTestId("library-open-" + id(4)).click();
  await page
    .getByTestId("reader-cover-actions")
    .getByRole("button", { name: "作品详情", exact: true })
    .click();
  await expect(page.getByTestId("library-version-updated-at")).toHaveText(
    "版本更新：2026-09-02",
  );
  await expect(page.getByTestId("library-added-at")).toContainText(
    "历史记录未知",
  );
  await page.getByTestId("library-detail-back").click();
  await page.getByTestId("search-input").fill("");
  await page.getByTestId("library-filter-all").click();
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({
    path: "visual-evidence/library-work-dates-wide.png",
  });
  await page.reload();
  await expect(page.getByTestId("library-sort")).toHaveValue("updated-asc");
  await expect(cards.first()).toHaveAttribute("data-library-id", id(3));
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.screenshot({
    path: "visual-evidence/library-work-dates-compact.png",
  });
});

test("PC directories use bounded rows, preserve full names during Unicode search and cancel folder selection", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1672, height: 941 });
  await installMock(page, { pcCount: 586, unicode: true, cancelChoose: true });
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({ path: "visual-evidence/pc-library.png" });
  await expect(page.getByTestId("library-grid")).toHaveAttribute(
    "data-total-items",
    "586",
  );
  expect(
    await page.getByTestId("library-grid").locator("article").count(),
  ).toBeLessThan(90);
  await page.getByTestId("search-input").fill("Café A [翻译甲]");
  await expect(page.getByTestId("library-grid")).toHaveAttribute(
    "data-total-items",
    "1",
  );
  await expect(page.getByTestId("library-card-" + id(1))).toContainText(
    "Cafe\u0301 A [翻译甲]",
  );
  await page.getByTestId("search-input").fill("");
  await expect(page.getByTestId("library-grid")).toHaveAttribute(
    "data-total-items",
    "586",
  );
  const choose = page.getByTestId("library-choose");
  await expect(choose).toBeEnabled();
  await choose.click();
  // Cancellation leaves the old catalog visible throughout the asynchronous
  // IPC call, so the unchanged count alone cannot prove that choice finished.
  await expect
    .poll(async () => (await commands(page, "library_choose")).length)
    .toBe(1);
  await expect(choose).toBeEnabled();
  await expect(page.getByTestId("library-grid")).toHaveAttribute(
    "data-total-items",
    "586",
  );
  expect(await commands(page, "library_scan")).toEqual([]);
});

test("mapping import finishes its scan without leaving a stale reading message in settings", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1672, height: 941 });
  await installMock(page, { pcCount: 3, selectCount: 60, holdNext: true });
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("settings-library").click();
  const panel = page.locator('[aria-labelledby="library-title"]');
  await page.getByTestId("library-import-paths").click();
  await expect
    .poll(() => page.evaluate(() => window.libraryTest.held))
    .toBe(true);
  await expect(panel.getByTestId("library-progress")).toContainText("正在读取");
  await expect(panel).toContainText("已迁移 2 条路径");
  await page.evaluate(() => window.libraryTest.release?.());
  await expect(panel.getByTestId("library-progress")).toContainText(
    "目录已读完 · 60 个电脑作品",
  );
  await expect(panel).not.toContainText("正在重新读取目录");
  await expect(panel.getByTestId("library-pause")).toHaveCount(0);
  await expect(page.getByTestId("library-import-paths")).toBeEnabled();
  expect(await commands(page, "library_import_paths")).toHaveLength(1);
  expect(
    (await commands(page, "library_scan")).map((call) => call.args.action),
  ).toEqual(["start", "next", "next", "next"]);
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({ path: "visual-evidence/zip-migration-complete.png" });
});

test("a selected root reads bounded batches, pauses after the current batch and explicitly resumes", async ({
  page,
}) => {
  await installMock(page, { selectCount: 60, holdNext: true });
  await page.getByTestId("library-choose").click();
  await expect
    .poll(() => page.evaluate(() => window.libraryTest.held))
    .toBe(true);
  await page.getByTestId("library-pause").click();
  await page.evaluate(() => window.libraryTest.release?.());
  await expect(page.getByTestId("library-resume")).toBeVisible();
  await expect(page.getByTestId("library-grid")).toHaveAttribute(
    "data-total-items",
    "40",
  );
  const paused = await commands(page, "library_scan");
  expect(paused.map((call) => call.args.action)).toEqual([
    "next",
    "next",
    "pause",
  ]);
  await page.getByTestId("library-resume").click();
  await expect(page.getByTestId("library-grid")).toHaveAttribute(
    "data-total-items",
    "60",
  );
  await expect
    .poll(() => page.evaluate(() => window.libraryTest.pc.phase))
    .toBe("complete");
});

test("scan errors retain the partial catalog across settings and require an explicit retry", async ({
  page,
}) => {
  await installMock(page, { selectCount: 60, failNextOnce: true });
  await page.getByTestId("library-choose").click();
  await expect(page.getByTestId("library-retry")).toBeVisible();
  await expect(page.getByTestId("library-grid")).toHaveAttribute(
    "data-total-items",
    "20",
  );
  const before = (await commands(page, "library_scan")).length;
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("nav-library").click();
  await expect(page.getByTestId("library-retry")).toBeVisible();
  expect(await commands(page, "library_scan")).toHaveLength(before);
  await page.getByTestId("library-retry").click();
  await expect(page.getByTestId("library-grid")).toHaveAttribute(
    "data-total-items",
    "60",
  );
  expect(
    (await commands(page, "library_scan")).some(
      (call) => call.args.action === "start",
    ),
  ).toBe(true);
});

test("decoded offscreen covers release while compressed covers survive scrolling, detail and settings", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1672, height: 941 });
  await installMock(page, { pcCount: 586, covers: true });
  const firstCover = page.getByTestId("library-cover-" + id(1)).locator("img");
  await expect(firstCover).toBeVisible();
  await expect
    .poll(() =>
      firstCover.evaluate(
        (image: HTMLImageElement) => image.complete && image.naturalWidth > 0,
      ),
    )
    .toBe(true);
  await page.getByTestId("library-grid").evaluate((grid) => {
    grid.closest("main")!.scrollTop = 12000;
  });
  await expect(page.getByTestId("library-card-" + id(1))).toHaveCount(0);
  await page.getByTestId("library-grid").evaluate((grid) => {
    grid.closest("main")!.scrollTop = 0;
  });
  await expect(firstCover).toBeVisible();
  await page.getByTestId("library-open-" + id(1)).click();
  await page
    .getByTestId("reader-cover-actions")
    .getByRole("button", { name: "作品详情", exact: true })
    .click();
  await expect(page.getByTestId("library-detail").locator("img")).toBeVisible();
  await page.getByTestId("library-detail-back").click();
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("nav-library").click();
  await expect(firstCover).toBeVisible();
  expect(
    (await commands(page, "library_cover")).filter(
      (call) => call.args.entryId === id(1),
    ),
  ).toHaveLength(1);
  expect(await page.evaluate(() => window.libraryTest.coverMax)).toBe(2);
  expect(await commands(page, "library_scan")).toEqual([]);
});

test("PC density changes and a detail return retain a deep catalog anchor", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1672, height: 941 });
  await installMock(page, { pcCount: 586 });
  await page.getByTestId("library-grid").evaluate((grid) => {
    grid.closest("main")!.scrollTop = 12000;
  });
  let anchor: string | null = null;
  await expect
    .poll(async () => {
      anchor = await page.getByTestId("library-grid").evaluate((grid) => {
        const main = grid.closest("main")!;
        const bounds = main.getBoundingClientRect();
        return (
          [...grid.querySelectorAll("article")]
            .find((article) => {
              const rectangle = article.getBoundingClientRect();
              return (
                rectangle.top >= bounds.top && rectangle.top < bounds.bottom
              );
            })
            ?.getAttribute("data-library-id") ?? null
        );
      });
      return anchor;
    })
    .toMatch(/^[a-f0-9]{64}$/);
  expect(anchor).not.toBe(id(1));
  for (const density of [5, 9, 7]) {
    await page
      .getByTestId("library-density-" + density)
      .evaluate((button: HTMLButtonElement) => button.click());
    // A still-visible old row is not evidence that the requested density has
    // committed. Wait for the new grid geometry before checking the anchor.
    await expect(page.getByTestId("library-grid")).toHaveAttribute(
      "data-density",
      String(density),
    );
    await expect(
      page.getByTestId("library-grid").locator(".source-virtual-row").first(),
    ).toHaveAttribute("data-columns", String(density));
    await expect(page.getByTestId("library-card-" + anchor)).toBeVisible();
    expect(
      await page.getByTestId("library-grid").locator("article").count(),
    ).toBeLessThan(90);
  }
  await page.getByTestId("library-open-" + anchor).click();
  await page
    .getByTestId("reader-cover-actions")
    .getByRole("button", { name: "作品详情", exact: true })
    .click();
  await expect(page.getByTestId("library-detail")).toBeVisible();
  await page.getByTestId("library-detail-back").click();
  await expect(page.getByTestId("library-card-" + anchor)).toBeVisible();
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("nav-library").click();
  await expect(page.getByTestId("library-card-" + anchor)).toBeVisible();
  expect(await commands(page, "library_scan")).toEqual([]);
});

test("retired phone and classification lists neither appear nor load, while PC state survives restart", async ({
  page,
}) => {
  await installMock(page, { pcCount: 3, phoneCount: 2833 });
  await expect(page.getByTestId("library-grid")).toHaveAttribute(
    "data-total-items",
    "3",
  );
  await expect(page.getByTestId("library-card-" + id(1))).toContainText(
    "已入库 · 电脑漫画库",
  );
  await expect(page.getByTestId("phone-tab")).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "本地书单", exact: true }),
  ).toHaveCount(0);
  expect(await commands(page, "phone_library_read")).toEqual([]);
  expect(await commands(page, "read_booklists")).toEqual([]);
  await page.getByTestId("library-open-" + id(1)).click();
  await page
    .getByTestId("reader-cover-actions")
    .getByRole("button", { name: "作品详情", exact: true })
    .click();
  await expect(page.getByTestId("library-source-id")).toHaveCount(0);
  await expect(page.getByTestId("library-link")).toHaveCount(0);
  const title = await page
    .getByTestId("library-detail")
    .getByRole("heading", { level: 1 })
    .innerText();
  await expect(page.getByTestId("phone-mark")).toHaveCount(0);
  await page.reload();
  await page.getByTestId("library-open-" + id(1)).click();
  await page
    .getByTestId("reader-cover-actions")
    .getByRole("button", { name: "作品详情", exact: true })
    .click();
  await expect(
    page.getByTestId("library-detail").getByRole("heading", { level: 1 }),
  ).toHaveText(title);
  expect(await commands(page, "library_link")).toEqual([]);
  await mkdir("visual-evidence", { recursive: true });
  await page.screenshot({ path: "visual-evidence/pc-library-detail.png" });
  await expect(page.getByTestId("library-detail-stock")).toContainText(
    "已入库 · 电脑漫画库",
  );
});
