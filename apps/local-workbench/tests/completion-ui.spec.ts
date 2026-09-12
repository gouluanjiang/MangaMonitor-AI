import { expect, test, type Page } from "@playwright/test";
import type { AccountSummary, SourceWork } from "../src/source-types.ts";
import type { LibrarySnapshot } from "../src/library-types.ts";
import type { PhoneLibrarySnapshot } from "../src/phone-library-types.ts";
import type {
  CompletionGroup,
  CompletionMember,
  CompletionSettings,
  CompletionView,
} from "../src/completion-types.ts";

// Browser IPC fixtures only. No website requests, credentials, phone transfer,
// real file operations or downloads are performed by this suite.
type Hooks = {
  calls: { command: string; args: Record<string, unknown> }[];
  view: CompletionView;
  settings: CompletionSettings;
  accounts: AccountSummary[];
  phone: PhoneLibrarySnapshot;
  library: LibrarySnapshot;
  holdNext: boolean;
  held: boolean;
  release?: () => void;
};
declare global {
  interface Window {
    completionTest: Hooks;
  }
}
const errors = new WeakMap<Page, string[]>();
test.use({ storageState: { cookies: [], origins: [] } });
test.beforeEach(async ({ page }) => {
  const collected: string[] = [];
  errors.set(page, collected);
  page.on("pageerror", (error) => collected.push(error.message));
  await page.setViewportSize({ width: 1672, height: 1020 });
});
test.afterEach(async ({ page }) => {
  expect(errors.get(page) ?? []).toEqual([]);
  const forbidden = await page.evaluate(() =>
    (window.completionTest?.calls ?? []).filter((call) =>
      /download_(prepare|confirm|control)|phone_library_(mark|unmark)|delete|promote|remove_file|move_file/.test(
        call.command,
      ),
    ),
  );
  expect(forbidden).toEqual([]);
});

function fixtures() {
  const accounts: AccountSummary[] = (["JM", "Pica"] as const).map(
    (source) => ({
      source,
      sessionId: "synthetic-" + source + "-1",
      accountId: "synthetic-account-" + source,
      displayName: "合成账号 " + source,
      state: "connected",
      remembered: false,
      errorCode: null,
    }),
  );
  const makeWork = (
    source: "JM" | "Pica",
    workId: string,
    title: string,
  ): SourceWork => ({
    source,
    workId,
    title,
    authors: ["合成作者"],
    description: null,
    tags: [],
    favorite: null,
    chapterCount: 1,
    pageCount: 20,
    coverAvailable: false,
  });
  const works = [
    makeWork("JM", "123", "01 旧作遗漏 [Chinese]"),
    makeWork("JM", "456", "02 已入库原版 [Japanese]"),
    makeWork("Pica", "0123456789abcdef01234567", "02 已入库汉化 [Chinese]"),
    makeWork("JM", "789", "03 待替换原版 [Japanese]"),
    makeWork("Pica", "1123456789abcdef01234567", "03 电脑已有汉化 [Chinese]"),
    makeWork("JM", "555", "04 身份待核对汉化 [Chinese]"),
    makeWork("JM", "999", "05 语言与作者待核对"),
  ];
  const hash = "f".repeat(64);
  const source = (
    work: SourceWork,
    language: "chinese" | "japanese" | "unknown",
    verified = true,
  ) => ({
    reference: { source: work.source, workId: work.workId },
    title: work.title,
    language,
    authorVerified: verified,
  });
  const makeGroup = (
    letter: string,
    work: SourceWork,
    status: CompletionGroup["status"],
  ): CompletionGroup => ({
    groupId: letter.repeat(64),
    title: work.title,
    authors: work.authors,
    status,
    reasons: [],
    sources: [source(work, "chinese")],
    phone: [],
    computer: [],
    eligible: null,
  });
  const missing = makeGroup("a", works[0], "missing");
  const owned = makeGroup("b", works[1], "owned_chinese");
  owned.sources = [source(works[1], "japanese"), source(works[2], "chinese")];
  owned.phone = [
    {
      member: { kind: "phone", name: "合成手机中文本 [Chinese]" },
      name: "合成手机中文本 [Chinese]",
      language: "chinese",
    },
  ];
  const downloaded = makeGroup("c", works[3], "translation_downloaded");
  downloaded.sources = [
    source(works[3], "japanese"),
    source(works[4], "chinese"),
  ];
  downloaded.phone = [
    {
      member: { kind: "phone", name: "待替换原版 [Japanese]" },
      name: "待替换原版 [Japanese]",
      language: "japanese",
    },
  ];
  downloaded.computer = [
    {
      member: { kind: "computer", itemId: "9".repeat(64) },
      name: "03 电脑已有汉化 [Chinese].zip",
      language: "chinese",
    },
  ];
  const review = makeGroup("d", works[5], "review_required");
  review.reasons = ["VERSION_IDENTITY_UNCONFIRMED"];
  const unknown = makeGroup("e", works[6], "review_required");
  unknown.sources = [source(works[6], "unknown", false)];
  unknown.reasons = ["SOURCE_LANGUAGE_OR_AUTHOR_UNCONFIRMED"];
  const view: CompletionView = {
    discovery: {
      scopes: accounts.map((a) => ({
        source: a.source,
        sessionId: a.sessionId!,
      })),
      revision: 4,
      run: null,
      authors: accounts.map((a) => ({
        source: a.source,
        author: "合成作者",
        state: "partial",
        lastAttemptAt: 1,
        lastCompleteAt: null,
        observedCount: 3,
        pagesRead: 1,
        errorCode: "SOURCE_UNAVAILABLE",
      })),
      records: works.map((work) => ({
        work,
        matchedAuthors: ["合成作者"],
        authorVerified: work.workId !== "999",
        observedAt: 1,
        scanId: "7".repeat(64),
      })),
    },
    completeness: {
      revision: 0,
      phoneRevision: 1,
      libraryRevision: 1,
      matchesRevision: 0,
      discoveryRevision: 4,
      evidenceHash: hash,
      groups: [missing, owned, downloaded, review, unknown],
    },
    automatic: {
      runId: null,
      phase: "idle",
      queued: 0,
      skipped: 0,
      errorCode: null,
    },
  };
  const phone: PhoneLibrarySnapshot = {
    revision: 1,
    importedNames: [
      "合成手机原版 [Japanese].zip",
      "合成手机中文本 [Chinese].zip",
    ],
    importedAt: 1,
    importFileName: "synthetic-phone.txt",
    manualEntries: [],
  };
  const library: LibrarySnapshot = {
    revision: 1,
    rootId: "8".repeat(64),
    rootPath: "C:\\Synthetic\\Comics",
    generation: 1,
    phase: "complete",
    freshness: "live",
    items: [],
    visited: 0,
    skipped: 0,
    updatedAt: 1,
    errorCode: null,
  };
  return { accounts, view, phone, library };
}

async function install(page: Page) {
  await page.addInitScript((fixture: ReturnType<typeof fixtures>) => {
    const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;
    const hooks: Hooks = (window.completionTest = {
      ...fixture,
      calls: [],
      settings: { revision: 0, families: [], languages: [] },
      holdNext: false,
      held: false,
    });
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
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: {
        invoke: async (command: string, args: Record<string, unknown> = {}) => {
          hooks.calls.push({ command, args: clone(args) });
          if (command === "read_preferences") return clone(preferences);
          if (command === "read_booklists")
            return { revision: 0, value: { version: 1, lists: [] } };
          if (command === "source_accounts") return clone(hooks.accounts);
          if (command === "source_matches_read")
            return { revision: 0, pairs: [] };
          if (command === "phone_library_read") return clone(hooks.phone);
          if (command === "library_read") return clone(hooks.library);
          if (command === "jm_download_read") return { revision: 0, tasks: [] };
          if (command === "source_following")
            return {
              source: args.source,
              sessionId: args.sessionId,
              revision: 0,
              works: [],
              authors: ["合成作者"],
            };
          if (command === "source_catalog")
            return {
              source: args.source,
              sessionId: args.sessionId,
              snapshot: null,
              completeSnapshot: null,
            };
          if (command === "source_cover")
            return {
              source: args.source,
              sessionId: args.sessionId,
              workId: args.workId,
              dataUrl: null,
            };
          if (command === "source_query")
            return {
              source: args.source,
              sessionId: args.sessionId,
              items: hooks.view.discovery.records
                .filter(
                  (r) =>
                    r.work.source === args.source &&
                    r.work.workId === args.query,
                )
                .map((r) => r.work),
              page: 1,
              total: 1,
              pages: 1,
              hasMore: false,
              folders: [],
            };
          if (command === "completeness_read") {
            const result = clone(hooks.view);
            if (hooks.holdNext) {
              hooks.holdNext = false;
              hooks.held = true;
              await new Promise<void>((resolve) => {
                hooks.release = resolve;
              });
            }
            return result;
          }
          if (command === "completeness_start") {
            hooks.view.discovery.run = {
              id: "6".repeat(64),
              phase: "checking",
              currentAuthor: "合成作者",
              currentSource: "JM",
              currentPage: 1,
              requestsUsed: 0,
              completedScopes: 0,
              totalScopes: 2,
              errorCode: null,
            };
            hooks.view.automatic = {
              runId: "6".repeat(64),
              phase: args.automatic ? "waiting" : "idle",
              queued: 0,
              skipped: 0,
              errorCode: null,
            };
            return clone(hooks.view);
          }
          if (command === "completeness_cancel") {
            hooks.view.discovery.run!.phase = "cancelled";
            hooks.view.automatic.phase = "cancelled";
            return;
          }
          if (command === "completeness_settings_read")
            return clone(hooks.settings);
          if (command === "completeness_family_confirm") {
            if (args.revision !== hooks.settings.revision)
              throw { code: "REVISION_CONFLICT" };
            const members = clone(args.members as CompletionMember[]);
            hooks.settings = {
              ...hooks.settings,
              revision: hooks.settings.revision + 1,
              families: [{ id: "2".repeat(64), members }],
            };
            hooks.view.completeness.revision = hooks.settings.revision;
            const group = hooks.view.completeness.groups.find(
              (g) => g.groupId === "d".repeat(64),
            )!;
            group.phone = members
              .filter((m) => m.kind === "phone")
              .map((member) => ({
                member,
                name: member.name,
                language: "japanese",
              }));
            group.status = "translation_available";
            group.reasons = [];
            group.eligible = {
              groupId: group.groupId,
              reference: group.sources[0].reference,
              kind: "translation",
              evidenceHash: hooks.view.completeness.evidenceHash,
            };
            return clone(hooks.settings);
          }
          if (command === "completeness_language_set") {
            if (args.revision !== hooks.settings.revision)
              throw { code: "REVISION_CONFLICT" };
            const member = args.member as CompletionMember;
            hooks.settings.revision++;
            hooks.view.completeness.revision = hooks.settings.revision;
            if (member.kind === "phone" && args.language === "chinese") {
              const group = hooks.view.completeness.groups.find(
                (g) => g.groupId === "d".repeat(64),
              )!;
              group.phone[0].language = "chinese";
              group.status = "owned_chinese";
              group.eligible = null;
            }
            return clone(hooks.settings);
          }
          throw { code: "SYNTHETIC_UNSUPPORTED_COMMAND" };
        },
      },
    });
  }, fixtures());
  await page.goto("/");
  await page.getByTestId("nav-completion").click();
  await expect(page.getByTestId("completion-panel")).toBeVisible();
  await expect(
    page.getByTestId("completion-group-" + "a".repeat(64)),
  ).toBeVisible();
}

async function calls(page: Page, command: string) {
  return page.evaluate(
    (value) =>
      window.completionTest.calls.filter((call) => call.command === value),
    command,
  );
}

test("opening author completion keeps old omissions, Chinese PC copies and uncertain candidates visible without starting work", async ({
  page,
}) => {
  await install(page);
  const panel = page.getByTestId("completion-panel");
  await expect(panel).toContainText("01 旧作遗漏");
  await expect(panel).toContainText("汉化已下载 · 待替换");
  await expect(panel).toContainText("需要核对");
  await expect(
    page.getByTestId("completion-group-" + "b".repeat(64)),
  ).toHaveCount(0);
  await page.getByRole("combobox", { name: "补全状态" }).selectOption("all");
  const owned = page.getByTestId("completion-group-" + "b".repeat(64));
  await expect(owned).toContainText("汉化已入库");
  await expect(owned).toContainText("JM · Pica");
  await expect(owned.getByRole("button", { name: "准备下载" })).toHaveCount(0);
  await page.getByRole("button", { name: "刷新入库状态", exact: true }).click();
  await expect
    .poll(async () =>
      (await calls(page, "completeness_read")).some(
        (call) => call.args.recheckFiles === true,
      ),
    )
    .toBe(true);
  expect(await calls(page, "completeness_start")).toEqual([]);
  expect(await calls(page, "completeness_family_confirm")).toEqual([]);
});

test("only an explicit check sends automatic permission, author union and current destination; stop cancels that run", async ({
  page,
}) => {
  await install(page);
  expect(await calls(page, "completeness_start")).toEqual([]);
  await page.getByTestId("completion-start").click();
  await expect(
    page.getByRole("button", { name: "停止本次检查与自动下载" }),
  ).toBeVisible();
  await expect
    .poll(async () => (await calls(page, "completeness_start")).length)
    .toBe(1);
  const start = (await calls(page, "completeness_start"))[0].args;
  expect(start).toEqual({
    scopes: fixtures().view.discovery.scopes,
    authors: [],
    automatic: true,
    rootId: "8".repeat(64),
    generation: 1,
  });
  await page.getByRole("button", { name: "停止本次检查与自动下载" }).click();
  await expect(page.getByTestId("completion-panel")).toContainText(
    "本次检查已停止",
  );
  expect((await calls(page, "completeness_cancel"))[0].args).toEqual({
    runId: "6".repeat(64),
  });
  await page
    .getByRole("checkbox", { name: "发现对应汉化后自动下载到电脑" })
    .uncheck();
  await page.getByTestId("completion-start").click();
  await expect
    .poll(async () => (await calls(page, "completeness_start")).length)
    .toBe(2);
  expect((await calls(page, "completeness_start"))[1].args.automatic).toBe(
    false,
  );
});

test("an explicit phone-name relation and language correction update projection without a phone write or automatic start", async ({
  page,
}) => {
  await install(page);
  const group = page.getByTestId("completion-group-" + "d".repeat(64));
  await group.getByRole("button", { name: "核对版本" }).click();
  const dialog = page.getByRole("dialog", { name: "核对作品版本" });
  await dialog.getByRole("checkbox", { name: /JM · 555/ }).check();
  await dialog
    .getByRole("textbox", { name: "搜索手机名单" })
    .fill("合成手机原版");
  await dialog
    .getByRole("checkbox", { name: "合成手机原版 [Japanese].zip", exact: true })
    .check();
  await dialog
    .getByRole("button", { name: "确认所选版本属于同一作品" })
    .click();
  await expect(group).toContainText("发现汉化 · 待下载");
  const relation = (await calls(page, "completeness_family_confirm"))[0].args;
  expect(relation).toEqual({
    revision: 0,
    members: [
      { kind: "source", reference: { source: "JM", workId: "555" } },
      { kind: "phone", name: "合成手机原版 [Japanese].zip" },
    ],
  });
  await page.getByRole("combobox", { name: "补全状态" }).selectOption("all");
  await dialog
    .getByRole("combobox", {
      name: "语言：合成手机原版 [Japanese].zip",
      exact: true,
    })
    .selectOption("chinese");
  await expect(group).toContainText("汉化已入库");
  expect((await calls(page, "completeness_language_set"))[0].args).toEqual({
    revision: 1,
    member: { kind: "phone", name: "合成手机原版 [Japanese].zip" },
    language: "chinese",
  });
  expect(await calls(page, "completeness_start")).toEqual([]);
  expect(
    await page.evaluate(() => window.completionTest.phone.importedNames),
  ).toEqual(fixtures().phone.importedNames);
});

test("a delayed old-account response cannot replace the newly connected account view", async ({
  page,
}) => {
  await install(page);
  await page.evaluate(() => {
    window.completionTest.holdNext = true;
  });
  await page.getByRole("button", { name: "刷新入库状态", exact: true }).click();
  await expect
    .poll(() => page.evaluate(() => window.completionTest.held))
    .toBe(true);
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("settings-accounts").click();
  await page.evaluate(() => {
    const hooks = window.completionTest;
    hooks.accounts = hooks.accounts.map((account) => ({
      ...account,
      sessionId: "synthetic-" + account.source + "-2",
    }));
    hooks.view.discovery.scopes = hooks.accounts.map((account) => ({
      source: account.source,
      sessionId: account.sessionId!,
    }));
    hooks.view.discovery.records[0].work.title = "新账号目录";
    hooks.view.completeness.groups[0].title = "新账号目录";
    hooks.view.completeness.groups[0].sources[0].title = "新账号目录";
  });
  await page
    .getByRole("button", { name: "重新读取账号状态", exact: true })
    .click();
  await expect
    .poll(
      async () =>
        (await calls(page, "source_accounts")).filter(
          (call) => call.args.refresh === true,
        ).length,
    )
    .toBeGreaterThan(0);
  await page.getByTestId("nav-completion").click();
  await expect(page.getByTestId("completion-panel")).toContainText(
    "新账号目录",
  );
  await page.evaluate(() => window.completionTest.release?.());
  await expect(page.getByTestId("completion-panel")).not.toContainText(
    "01 旧作遗漏",
  );
  await expect(page.getByTestId("completion-panel")).toContainText(
    "新账号目录",
  );
  expect(await calls(page, "completeness_start")).toEqual([]);
});
