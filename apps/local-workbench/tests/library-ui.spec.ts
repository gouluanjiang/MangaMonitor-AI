import { expect, test, type Page } from "@playwright/test";
import type { LibrarySnapshot } from "../src/library-types.ts";
import type { PhoneLibrarySnapshot } from "../src/phone-library-types.ts";

// Synthetic Chromium/native-IPC interaction only. This suite never reads a real
// TXT, PC directory, phone, archive, credential, website or production inventory.
test.use({ storageState: { cookies: [], origins: [] } });
type Call = { command: string; args: Record<string, unknown> };
type Options = {
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
};
type Hooks = {
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
    (window.libraryTest?.calls ?? []).filter((call) =>
      /download|delete|remove_file|move_file|source_set_favorite|production|promote/.test(
        call.command,
      ),
    ),
  );
  expect(forbidden).toEqual([]);
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
        tags: [],
        bytes: 4096,
        modifiedAt: 1800000000000,
        pageCount: null,
        coverAvailable: Boolean(options.covers),
        state: "indexed",
        errorCode: null,
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
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: {
        invoke: async (command: string, args: Record<string, unknown> = {}) => {
          hooks.calls.push({ command, args: clone(args) });
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
const commands = (page: Page, command: string) =>
  page.evaluate(
    (command) =>
      window.libraryTest.calls.filter((call) => call.command === command),
    command,
  );

test("phone-only 2833-name inventory is browsable and searchable without a PC folder or startup scan", async ({
  page,
}) => {
  await installMock(page, { phoneCount: 2833 });
  await page.getByTestId("phone-tab").click();
  await expect(page.getByTestId("phone-library-grid")).toHaveAttribute(
    "data-total-items",
    "2833",
  );
  expect(
    await page.getByTestId("phone-library-grid").locator("article").count(),
  ).toBeLessThan(90);
  await page.getByTestId("search-input").fill("合成手机作品 2833");
  await expect(page.getByTestId("phone-library-grid")).toContainText(
    "合成手机作品 2833",
  );
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("nav-library").click();
  await page.getByTestId("phone-tab").click();
  await expect(page.getByTestId("phone-library-grid")).toBeVisible();
  expect(await commands(page, "library_scan")).toEqual([]);
  expect(await commands(page, "library_choose")).toEqual([]);
  expect(await commands(page, "phone_library_import")).toEqual([]);
  expect(await commands(page, "library_cover")).toEqual([]);
});

test("PC directories use bounded rows, preserve full names during Unicode search and cancel folder selection", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1672, height: 941 });
  await installMock(page, { pcCount: 586, unicode: true, cancelChoose: true });
  await page.getByTestId("pc-tab").click();
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

test("a selected root reads bounded batches, pauses after the current batch and explicitly resumes", async ({
  page,
}) => {
  await installMock(page, { selectCount: 60, holdNext: true });
  await page.getByTestId("pc-tab").click();
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
  await page.getByTestId("pc-tab").click();
  await page.getByTestId("library-choose").click();
  await expect(page.getByTestId("library-retry")).toBeVisible();
  await expect(page.getByTestId("library-grid")).toHaveAttribute(
    "data-total-items",
    "20",
  );
  const before = (await commands(page, "library_scan")).length;
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("nav-library").click();
  await page.getByTestId("pc-tab").click();
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

test("manual phone marks and source links survive restart while imported TXT replaces only imported names", async ({
  page,
}) => {
  await installMock(page, {
    pcCount: 2,
    phoneCount: 1,
    importedNames: ["合成更新手机作品.rar"],
    cancelImportOnce: true,
  });
  await page.getByTestId("pc-tab").click();
  await page.getByTestId("library-open-" + id(1)).click();
  await expect(page.getByTestId("library-detail")).toBeVisible();
  await page.getByTestId("library-source").selectOption("JM");
  await page.getByTestId("library-source-id").fill("JM413751");
  await page.getByTestId("library-link").click();
  await expect
    .poll(() =>
      page.evaluate(() => window.libraryTest.pc.items[0].sourceRef?.workId),
    )
    .toBe("413751");
  await page.getByTestId("phone-mark").click();
  await expect(page.getByTestId("library-detail")).toContainText("已入库");
  expect(await page.evaluate(() => window.libraryTest.pc.items.length)).toBe(2);
  await page.getByTestId("library-detail-back").click();
  await page.getByTestId("phone-tab").click();
  await expect(page.getByTestId("phone-library-grid")).toHaveAttribute(
    "data-total-items",
    "2",
  );
  await page.getByTestId("phone-library-import").click();
  await expect(page.getByTestId("phone-library-grid")).toHaveAttribute(
    "data-total-items",
    "2",
  );
  await page.getByTestId("phone-library-import").click();
  await expect(page.getByTestId("phone-library-grid")).toContainText(
    "合成更新手机作品",
  );
  await expect(page.getByTestId("phone-library-grid")).toContainText(
    "合成电脑作品 0001",
  );
  await expect(page.getByTestId("phone-library-grid")).not.toContainText(
    "合成手机作品 0001",
  );
  await page.reload();
  await page.getByTestId("phone-tab").click();
  await expect(page.getByTestId("phone-library-grid")).toContainText(
    "合成电脑作品 0001",
  );
  await page.getByTestId("pc-tab").click();
  await page.getByTestId("library-open-" + id(1)).click();
  await expect(page.getByTestId("library-detail")).toContainText("已入库");
  await expect(page.getByTestId("library-detail")).toContainText("413751");
  expect(await commands(page, "library_scan")).toEqual([]);
  expect(
    await page.evaluate(() =>
      window.libraryTest.pc.items.map((entry) => entry.format),
    ),
  ).toEqual(["directory", "directory"]);
});

test("removing a manual phone mark leaves imported evidence and every PC work intact", async ({
  page,
}) => {
  await installMock(page, {
    pcCount: 1,
    importedNames: ["合成电脑作品 0001.zip"],
  });
  await page.getByTestId("pc-tab").click();
  await page.getByTestId("library-open-" + id(1)).click();
  await page.getByTestId("phone-mark").click();
  await page.getByTestId("library-detail-back").click();
  await page.getByTestId("phone-tab").click();
  await page.getByTestId("phone-library-import").click();
  await page.getByTestId("phone-unmark-" + id(900000)).click();
  await expect(page.getByTestId("phone-library-grid")).toHaveAttribute(
    "data-total-items",
    "1",
  );
  await expect(page.getByTestId("phone-library-grid")).toContainText(
    "合成电脑作品 0001",
  );
  await page.getByTestId("pc-tab").click();
  await expect(page.getByTestId("library-card-" + id(1))).toContainText(
    "已入库",
  );
  expect(await page.evaluate(() => window.libraryTest.pc.items.length)).toBe(1);
});

test("decoded offscreen covers release while compressed covers survive scrolling, detail and settings", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1672, height: 941 });
  await installMock(page, { pcCount: 586, covers: true });
  await page.getByTestId("pc-tab").click();
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
  await expect(page.getByTestId("library-detail").locator("img")).toBeVisible();
  await page.getByTestId("library-detail-back").click();
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("nav-library").click();
  await page.getByTestId("pc-tab").click();
  await expect(firstCover).toBeVisible();
  expect(
    (await commands(page, "library_cover")).filter(
      (call) => call.args.entryId === id(1),
    ),
  ).toHaveLength(1);
  expect(await page.evaluate(() => window.libraryTest.coverMax)).toBe(1);
  expect(await commands(page, "library_scan")).toEqual([]);
});

test("PC density changes and a detail return retain a deep catalog anchor", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1672, height: 941 });
  await installMock(page, { pcCount: 586 });
  await page.getByTestId("pc-tab").click();
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
  await expect(page.getByTestId("library-detail")).toBeVisible();
  await page.getByTestId("library-detail-back").click();
  await expect(page.getByTestId("library-card-" + anchor)).toBeVisible();
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("nav-library").click();
  await expect(page.getByTestId("library-card-" + anchor)).toBeVisible();
  expect(await commands(page, "library_scan")).toEqual([]);
});

test("a failed phone TXT update keeps the prior inventory until an explicit successful import", async ({
  page,
}) => {
  await installMock(page, { phoneCount: 2, failImportOnce: true });
  await page.getByTestId("phone-tab").click();
  await page.getByTestId("phone-library-import").click();
  await expect(page.getByRole("alert")).toContainText("原名单保留");
  await expect(page.getByTestId("phone-library-grid")).toHaveAttribute(
    "data-total-items",
    "2",
  );
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("nav-library").click();
  await page.getByTestId("phone-tab").click();
  expect(await commands(page, "phone_library_import")).toHaveLength(1);
  await page.getByTestId("phone-library-import").click();
  await expect(page.getByTestId("phone-library-grid")).toHaveAttribute(
    "data-total-items",
    "1",
  );
  await expect(page.getByTestId("phone-library-grid")).toContainText(
    "合成更新手机作品",
  );
  expect(await commands(page, "library_scan")).toEqual([]);
});

test("an unreadable phone index stays unverified until a user retries reading it", async ({
  page,
}) => {
  await installMock(page, { pcCount: 1, phoneCount: 1, failPhoneRead: true });
  await page.getByTestId("pc-tab").click();
  await expect(page.getByTestId("library-card-" + id(1))).toContainText(
    "待核对",
  );
  await page.getByTestId("library-open-" + id(1)).click();
  await expect(page.getByTestId("library-detail-stock")).toContainText(
    "待核对",
  );
  await expect(page.getByTestId("phone-mark")).toBeDisabled();
  await page.getByTestId("library-detail-back").click();
  await page.getByTestId("phone-tab").click();
  await expect(page.getByTestId("phone-library-import")).toBeDisabled();
  await page.evaluate(() => {
    window.libraryTest.phoneReadBlocked = false;
  });
  await page.getByTestId("phone-library-read").click();
  await expect(page.getByTestId("phone-library-grid")).toHaveAttribute(
    "data-total-items",
    "1",
  );
  await page.getByTestId("pc-tab").click();
  await expect(page.getByTestId("library-card-" + id(1))).toContainText(
    "已下载",
  );
  expect(await commands(page, "phone_library_mark")).toEqual([]);
});

test("same-named PC works do not inherit or revoke another source's explicit phone mark", async ({
  page,
}) => {
  await installMock(page, { pcCount: 2, namespaceMarks: true });
  await page.getByTestId("pc-tab").click();
  await expect(page.getByTestId("library-card-" + id(1))).toContainText(
    "已入库",
  );
  await expect(page.getByTestId("library-card-" + id(2))).toContainText(
    "已下载",
  );
  await page.getByTestId("library-open-" + id(2)).click();
  await expect(page.getByTestId("library-reference")).toContainText("Pica");
  await expect(page.getByTestId("phone-unmark-" + id(900000))).toHaveCount(0);
  await page.getByTestId("library-detail-back").click();
  await page.getByTestId("library-open-" + id(1)).click();
  await expect(page.getByTestId("library-reference")).toContainText("JM");
  await page.getByTestId("phone-unmark-" + id(900000)).click();
  await expect(page.getByTestId("library-detail-stock")).toContainText(
    "已下载",
  );
  expect(await page.evaluate(() => window.libraryTest.pc.items.length)).toBe(2);
  expect(await commands(page, "phone_library_unmark")).toHaveLength(1);
});
