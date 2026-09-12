import { expect, test, type Locator, type Page } from "@playwright/test";
import type {
  Booklist,
  BooklistsDocument,
  WorkReference,
} from "../src/booklists.ts";

// These tests exercise the Chromium browser preview and its browser persistence.
// They do not run native WebView commands or replace Rust/native durability tests.
test.use({ storageState: { cookies: [], origins: [] } });

const BOOKLISTS_KEY = "mangamonitor.workbench.booklists.v1";
const DEMO_KEY = "mangamonitor.workbench.demo.v1";
const errorsByPage = new WeakMap<Page, string[]>();
const fixtureIds = Array.from(
  { length: 100 },
  (_, index) => "fixture-" + String(index + 1).padStart(3, "0"),
);

declare global {
  interface Window {
    booklistTestHooks?: {
      writeAttempts: number;
      lockRequests: number;
      restoreWrites?: () => void;
      releaseWrite?: () => void;
    };
  }
}

test.beforeEach(async ({ page }) => {
  const errors: string[] = [];
  errorsByPage.set(page, errors);
  page.on("pageerror", (error) => errors.push(error.stack ?? error.message));
  await page.goto("/");
  await expect(page.getByTestId("demo-label")).toContainText("交互样例");
  await expect(page.getByTestId("demo-label")).toContainText("模拟数据");
});

test.afterEach(async ({ page }) => {
  expect(
    errorsByPage.get(page) ?? [],
    "browser preview runtime errors",
  ).toEqual([]);
});

function booklistKey(fixture?: string) {
  return BOOKLISTS_KEY + (fixture ? "." + fixture : "");
}

function reference(source: "JM" | "Pica", workId: string): WorkReference {
  return { source, workId };
}

function list(
  id: string,
  name: string,
  members: WorkReference[] = [],
): Booklist {
  return {
    id,
    name,
    createdAt: 1_800_000_000_000,
    updatedAt: 1_800_000_000_000,
    archived: false,
    members,
  };
}

async function rawStorage(page: Page, key: string): Promise<string | null> {
  return page.evaluate((storageKey) => localStorage.getItem(storageKey), key);
}

async function readBooklists(
  page: Page,
  fixture?: string,
): Promise<BooklistsDocument> {
  const raw = await rawStorage(page, booklistKey(fixture));
  expect(raw, "a completed write must exist in browser storage").not.toBeNull();
  return JSON.parse(raw!) as BooklistsDocument;
}

async function seedRaw(page: Page, raw: string, fixture?: string) {
  await page.evaluate(({ key, value }) => localStorage.setItem(key, value), {
    key: booklistKey(fixture),
    value: raw,
  });
  await page.reload();
  await expect(page.getByTestId("demo-label")).toContainText("交互样例");
}

async function seedLists(page: Page, lists: Booklist[], fixture?: string) {
  await seedRaw(page, JSON.stringify({ version: 1, lists }), fixture);
}

async function openBooklists(page: Page, id?: string) {
  await page.getByTestId("nav-library").click();
  await page.getByRole("button", { name: "本地书单", exact: true }).click();
  await expect(page.getByTestId("booklist-controls")).toBeVisible();
  if (id) await page.getByTestId("booklist-select").selectOption(id);
}

async function openDiscovery(page: Page) {
  await page.getByTestId("nav-discovery").click();
  await page.getByRole("button", { name: "全部作品", exact: true }).click();
}

async function pauseDemo(page: Page) {
  await page.getByTestId("nav-queue").click();
  await page.getByTestId("pause-queue").click();
  await expect(page.getByTestId("pause-queue")).toContainText("继续队列");
  await expect
    .poll(async () => JSON.parse((await rawStorage(page, DEMO_KEY))!).paused)
    .toBe(true);
  return rawStorage(page, DEMO_KEY);
}

async function createList(page: Page, name: string) {
  await openBooklists(page);
  await page.getByTestId("booklist-create").click();
  await page.getByTestId("booklist-name").fill(name);
  await page.getByTestId("booklist-save").click();
  await expect(page.getByTestId("booklist-editor")).toBeHidden();
  await expect(page.getByTestId("booklist-select")).not.toHaveValue("");
  const id = await page.getByTestId("booklist-select").inputValue();
  expect(
    (await readBooklists(page)).lists.find((item) => item.id === id)?.name,
  ).toBe(name.trim());
  return id;
}

async function addFromDetail(page: Page, workId: string, targets: string[]) {
  await openDiscovery(page);
  await page.getByTestId("open-" + workId).click();
  await page.getByTestId("detail-booklist").click();
  for (const id of targets)
    await page.getByTestId("booklist-target-" + id).check();
  await page.getByTestId("booklist-picker-save").click();
  await expect(page.getByTestId("booklist-picker")).toBeHidden();
}

async function expectNoOverflow(page: Page) {
  await expect
    .poll(() =>
      page.evaluate(() =>
        Math.max(
          document.documentElement.scrollWidth -
            document.documentElement.clientWidth,
          ...Array.from(
            document.querySelectorAll("main, dialog[open]"),
            (element) => element.scrollWidth - element.clientWidth,
          ),
        ),
      ),
    )
    .toBeLessThanOrEqual(1);
}

async function positionAnchor(card: Locator) {
  await card.evaluate((element) => {
    const main = element.closest("main")!;
    main.scrollTop +=
      element.getBoundingClientRect().top - main.getBoundingClientRect().top;
    const toolbar = main.querySelector(".library-toolbar")!;
    main.scrollTop -=
      toolbar.getBoundingClientRect().bottom -
      main.getBoundingClientRect().top +
      8;
  });
  return card.evaluate(
    (element) =>
      element.getBoundingClientRect().top -
      element.closest("main")!.getBoundingClientRect().top,
  );
}

async function expectAnchor(card: Locator, offset: number) {
  await expect
    .poll(() =>
      card.evaluate(
        (element, expected) =>
          Math.abs(
            element.getBoundingClientRect().top -
              element.closest("main")!.getBoundingClientRect().top -
              expected,
          ),
        offset,
      ),
    )
    .toBeLessThanOrEqual(3);
}

test("create, rename, reload, archive and restore a booklist with retained members", async ({
  page,
}) => {
  const id = await createList(page, "  旅行书单  ");
  await addFromDetail(page, "rain", [id]);
  await openBooklists(page, id);
  await page.getByTestId("booklist-rename").click();
  await page.getByTestId("booklist-name").fill("雨天旅行");
  await page.getByTestId("booklist-save").click();
  await expect(page.getByTestId("booklist-editor")).toBeHidden();
  await page.reload();
  await openBooklists(page, id);
  await expect(
    page.getByTestId("booklist-select").locator("option:checked"),
  ).toContainText("雨天旅行");
  await expect(page.getByTestId("card-rain")).toBeVisible();
  await page.getByTestId("booklist-archive").click();
  await expect(page.getByTestId("booklist-empty")).toBeVisible();
  await expect
    .poll(async () => (await readBooklists(page)).lists[0].archived)
    .toBe(true);
  await page.reload();
  await openBooklists(page);
  await page.getByTestId("booklist-archived-toggle").click();
  await expect(page.getByTestId("archived-booklist-" + id)).toContainText(
    "雨天旅行",
  );
  await page.getByTestId("booklist-restore-" + id).click();
  await expect(page.getByTestId("booklist-select")).toHaveValue(id);
  await expect(page.getByTestId("card-rain")).toBeVisible();
  const restored = (await readBooklists(page)).lists[0];
  expect(restored.archived).toBe(false);
  expect(restored.members).toEqual([reference("JM", "rain")]);
});

test("mixed-source owned and review works can be organized while download eligibility stays separate", async ({
  page,
}) => {
  const queueBefore = await pauseDemo(page);
  const id = await createList(page, "跨来源整理");
  await openDiscovery(page);
  await page.getByTestId("toggle-selection").click();
  for (const workId of ["rain", "bookshop", "summer", "echo"]) {
    await page.getByTestId("select-" + workId).check();
  }
  await expect(page.locator(".selected-count")).toHaveText("4");
  await page.getByTestId("batch-download").click();
  const download = page.getByTestId("confirm-dialog");
  await expect(download.locator(".confirmation-work")).toHaveCount(2);
  await expect(download).toContainText("雨停之前");
  await expect(download).toContainText("雾中书店");
  await expect(download).not.toContainText("夏日观测");
  await expect(download).not.toContainText("星光回声");
  await download.getByRole("button", { name: "取消", exact: true }).click();
  await page.getByTestId("select-rain").uncheck();
  await page.getByTestId("select-bookshop").uncheck();
  await expect(page.getByTestId("batch-download")).toBeDisabled();
  await expect(page.getByTestId("batch-booklist")).toBeEnabled();
  await page.getByTestId("select-rain").check();
  await page.getByTestId("select-bookshop").check();
  await page.getByTestId("batch-booklist").click();
  await page.getByTestId("booklist-target-" + id).check();
  await page.getByTestId("booklist-picker-save").click();
  await expect(page.getByTestId("booklist-picker")).toBeHidden();
  const members = (await readBooklists(page)).lists[0].members;
  expect(members.map((item) => item.source + ":" + item.workId).sort()).toEqual(
    ["JM:echo", "JM:rain", "Pica:bookshop", "Pica:summer"],
  );
  expect(await rawStorage(page, DEMO_KEY)).toBe(queueBefore);
  await openBooklists(page, id);
  await expect(
    page.getByTestId("cover-grid").locator("[data-work-id]"),
  ).toHaveCount(4);
  await expect(page.getByTestId("card-summer")).toContainText("已入库");
  await expect(page.getByTestId("card-echo")).toContainText("待复核");
});

test("detail membership supports multiple booklists and disables duplicate additions", async ({
  page,
}) => {
  const ids = [];
  for (const name of ["第一书单", "第二书单", "第三书单"])
    ids.push(await createList(page, name));
  await addFromDetail(page, "rain", ids.slice(0, 2));
  await page.getByTestId("detail-booklist").click();
  for (const id of ids.slice(0, 2)) {
    await expect(page.getByTestId("booklist-target-" + id)).toBeDisabled();
  }
  await expect(page.getByTestId("booklist-picker-save")).toBeDisabled();
  await page.getByTestId("booklist-target-" + ids[2]).check();
  await page.getByTestId("booklist-picker-save").click();
  await expect(page.getByTestId("booklist-picker")).toBeHidden();
  for (const saved of (await readBooklists(page)).lists) {
    expect(saved.members).toEqual([reference("JM", "rain")]);
  }
  await expect(page.getByTestId("detail-page")).toBeVisible();
});

test("removing booklist members leaves local inventory and the paused demo queue unchanged", async ({
  page,
}) => {
  const queueBefore = await pauseDemo(page);
  await seedLists(page, [
    list("mixed", "保留库存", [
      reference("Pica", "summer"),
      reference("Pica", "moon"),
      reference("JM", "rain"),
      reference("JM", "echo"),
    ]),
  ]);
  await openBooklists(page, "mixed");
  await page.getByTestId("toggle-selection").click();
  await page.getByTestId("select-summer").check();
  await page.getByTestId("select-rain").check();
  await page.getByTestId("remove-booklist-members").click();
  await expect(page.locator(".selection-bar")).toBeHidden();
  expect((await readBooklists(page)).lists[0].members).toEqual([
    reference("Pica", "moon"),
    reference("JM", "echo"),
  ]);
  expect(await rawStorage(page, DEMO_KEY)).toBe(queueBefore);
  await page.getByRole("button", { name: "全部作品", exact: true }).click();
  const grid = page.getByTestId("cover-grid");
  await expect(grid.locator('[data-work-id="summer"]')).toBeVisible();
  await expect(grid.locator('[data-work-id="moon"]')).toBeVisible();
});

test("booklist source, search, selection and density survive settings and detail visits", async ({
  page,
}) => {
  await seedLists(page, [
    list("context", "保留浏览位置", [
      reference("JM", "rain"),
      reference("Pica", "bookshop"),
      reference("Pica", "summer"),
    ]),
  ]);
  await openBooklists(page, "context");
  await page.getByLabel("来源筛选").selectOption("Pica");
  await page.getByTestId("search-input").fill("雾");
  await page.getByTestId("toggle-selection").click();
  await page.getByTestId("select-bookshop").check();
  await page.getByRole("button", { name: "每行 9 部", exact: true }).click();
  await expect(page.getByTestId("cover-grid")).toHaveAttribute(
    "data-density",
    "9",
  );
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("settings-appearance").click();
  await page.getByTestId("background-mode-A").click();
  await page.getByTestId("save-settings-page").click();
  await expect(page.locator(".settings-save-message")).toContainText(
    "外观已保存",
  );
  await expect(page.getByTestId("save-settings-page")).toBeDisabled();
  await page.getByTestId("nav-library").click();
  await expect(page.getByTestId("booklist-select")).toHaveValue("context");
  await expect(page.getByLabel("来源筛选")).toHaveValue("Pica");
  await expect(page.getByTestId("search-input")).toHaveValue("雾");
  await expect(page.getByTestId("select-bookshop")).toBeChecked();
  await expect(page.getByTestId("cover-grid")).toHaveAttribute(
    "data-density",
    "9",
  );
  await page.getByTestId("open-bookshop").click();
  await expect(page.getByTestId("nav-library")).toHaveAttribute(
    "aria-current",
    "page",
  );
  await page.getByTestId("back-library").click();
  await expect(page.getByTestId("booklist-select")).toHaveValue("context");
  await expect(page.getByTestId("select-bookshop")).toBeChecked();
  await expect(page.getByTestId("search-input")).toHaveValue("雾");
  await page.getByLabel("来源筛选").selectOption("JM");
  await expect(page.locator(".selection-bar")).toBeHidden();
});

test("100 real fixture members retain order, offscreen selection, anchors and narrow-window access", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1672, height: 941 });
  await page.goto("/?fixture=ready-100");
  await openDiscovery(page);
  await page.getByTestId("search-input").fill("示例作品");
  await page.getByTestId("toggle-selection").click();
  await page.getByTestId("select-all-works").click();
  await expect(page.locator(".selected-count")).toHaveText("100");
  await page.getByTestId("batch-booklist").click();
  await page.getByTestId("booklist-picker-name").fill("百部作品");
  await page.getByTestId("booklist-picker-save").click();
  await expect(page.getByTestId("booklist-picker")).toBeHidden();
  const saved = (await readBooklists(page, "ready-100")).lists[0];
  expect(saved.members.map((member) => member.workId)).toEqual(fixtureIds);
  await openBooklists(page, saved.id);
  const grid = page.getByTestId("cover-grid");
  await expect(grid.locator("[data-work-id]")).toHaveCount(100);
  await expect(grid.locator('img:not([loading="lazy"])')).toHaveCount(0);
  await page.getByRole("button", { name: "每行 5 部", exact: true }).click();
  await expect(grid).toHaveAttribute("data-density", "5");
  const rows = await grid.locator("[data-work-id]").evaluateAll((cards) =>
    cards.map((card) => ({
      id: card.getAttribute("data-work-id"),
      y: Math.round(card.getBoundingClientRect().top),
    })),
  );
  expect(rows.map((row) => row.id)).toEqual(fixtureIds);
  expect(new Set(rows.map((row) => row.y)).size).toBe(20);
  await page.getByTestId("toggle-selection").click();
  await page.getByTestId("select-all-works").click();
  const anchor = grid.locator('[data-work-id="fixture-046"]');
  const offset = await positionAnchor(anchor);
  await page.getByRole("button", { name: "每行 9 部", exact: true }).click();
  await expect(grid).toHaveAttribute("data-density", "9");
  await expectAnchor(anchor, offset);
  await expect(grid.locator('input[type="checkbox"]:checked')).toHaveCount(100);
  await page.getByTestId("open-fixture-046").click();
  await page.getByTestId("back-library").click();
  await expectAnchor(anchor, offset);
  await page.setViewportSize({ width: 390, height: 680 });
  await page.locator("main").evaluate((main) => {
    main.scrollTop = main.scrollHeight;
  });
  const lastBottom = await grid
    .locator('[data-work-id="fixture-100"]')
    .evaluate((element) => element.getBoundingClientRect().bottom);
  const barTop = await page
    .locator(".selection-bar")
    .evaluate((element) => element.getBoundingClientRect().top);
  expect(lastBottom).toBeLessThanOrEqual(barTop + 1);
  await expect(page.getByTestId("remove-booklist-members")).toBeInViewport();
  await expectNoOverflow(page);
  await page.reload();
  await openBooklists(page, saved.id);
  await expect(
    page.getByTestId("cover-grid").locator("[data-work-id]"),
  ).toHaveCount(100);
  await page.goto("/");
  await openBooklists(page);
  await expect(page.getByTestId("booklist-empty")).toBeVisible();
  expect(await rawStorage(page, BOOKLISTS_KEY)).toBeNull();
});

test("unknown and mismatched-source references remain visible and survive edits", async ({
  page,
}) => {
  const members = [
    reference("JM", "rain"),
    reference("Pica", "rain"),
    reference("Pica", "missing-record"),
  ];
  await seedLists(page, [list("unknown", "暂未取得资料", members)]);
  await openBooklists(page, "unknown");
  await expect(
    page.getByTestId("cover-grid").locator("[data-work-id]"),
  ).toHaveCount(1);
  const unavailable = page.getByTestId("unavailable-members");
  await expect(unavailable).toContainText("部分作品资料暂不可用（2）");
  await expect(unavailable).toContainText("Pica · rain");
  await expect(unavailable).toContainText("missing-record");
  await page.getByTestId("booklist-rename").click();
  await page.getByTestId("booklist-name").fill("保留未知引用");
  await page.getByTestId("booklist-save").click();
  await expect(page.getByTestId("booklist-editor")).toBeHidden();
  await page.reload();
  await openBooklists(page, "unknown");
  expect((await readBooklists(page)).lists[0].members).toEqual(members);
  await expect(page.getByTestId("unavailable-members")).toContainText(
    "missing-record",
  );
});

for (const sample of [
  { name: "corrupt JSON", raw: '{"version":1,"lists":' },
  {
    name: "future schema",
    raw: JSON.stringify({ version: 2, lists: [list("future", "未来数据")] }),
  },
]) {
  test(
    sample.name +
      " prevents creation and remains untouched after a failed reread",
    async ({ page }) => {
      await seedRaw(page, sample.raw);
      await openBooklists(page);
      await expect(page.getByTestId("booklists-error")).toContainText(
        "原数据已保留",
      );
      await expect(page.getByTestId("booklist-create")).toBeDisabled();
      await page.getByTestId("reload-booklists").click();
      await expect(page.getByTestId("booklist-create")).toBeDisabled();
      expect(await rawStorage(page, BOOKLISTS_KEY)).toBe(sample.raw);
      await openDiscovery(page);
      await page.getByTestId("open-rain").click();
      await expect(page.getByTestId("detail-booklist")).toBeDisabled();
      expect(await rawStorage(page, BOOKLISTS_KEY)).toBe(sample.raw);
    },
  );
}

test("a rejected booklist write retains the editor draft and can be retried", async ({
  page,
}) => {
  await openBooklists(page);
  await page.getByTestId("booklist-create").click();
  await page.getByTestId("booklist-name").fill("失败后重试");
  await page.evaluate((key) => {
    const original = Storage.prototype.setItem;
    const hooks = (window.booklistTestHooks = {
      writeAttempts: 0,
      lockRequests: 0,
    } as NonNullable<Window["booklistTestHooks"]>);
    hooks.restoreWrites = () => {
      Storage.prototype.setItem = original;
    };
    Storage.prototype.setItem = function (storageKey, value) {
      if (storageKey === key) {
        hooks.writeAttempts += 1;
        throw new DOMException("booklist test quota", "QuotaExceededError");
      }
      return original.call(this, storageKey, value);
    };
  }, BOOKLISTS_KEY);
  await page.getByTestId("booklist-save").click();
  const editor = page.getByTestId("booklist-editor");
  await expect(editor.getByRole("alert")).toContainText("草稿和选择已保留");
  await expect(page.getByTestId("booklist-name")).toHaveValue("失败后重试");
  await expect(page.getByTestId("booklist-save")).toBeEnabled();
  expect(await rawStorage(page, BOOKLISTS_KEY)).toBeNull();
  expect(
    await page.evaluate(() => window.booklistTestHooks?.writeAttempts),
  ).toBe(1);
  await page.evaluate(() => window.booklistTestHooks?.restoreWrites?.());
  await page.getByTestId("booklist-save").click();
  await expect(editor).toBeHidden();
  const document = await readBooklists(page);
  expect(document.lists).toHaveLength(1);
  expect(document.lists[0].name).toBe("失败后重试");
});

test("a delayed browser write stays pending and duplicate submissions produce one change", async ({
  page,
}) => {
  await openBooklists(page);
  await page.getByTestId("booklist-create").click();
  await page.getByTestId("booklist-name").fill("等待落盘");
  await page.evaluate((key) => {
    const request = navigator.locks.request;
    const hooks = (window.booklistTestHooks = {
      writeAttempts: 0,
      lockRequests: 0,
    } as NonNullable<Window["booklistTestHooks"]>);
    navigator.locks.request = ((
      ...args: Parameters<LockManager["request"]>
    ) => {
      const run = () =>
        Reflect.apply(request, navigator.locks, args) as Promise<unknown>;
      if (args[0] !== key) return run();
      hooks.lockRequests += 1;
      return new Promise((resolve, reject) => {
        hooks.releaseWrite = () => {
          delete hooks.releaseWrite;
          void run().then(resolve, reject);
        };
      });
    }) as LockManager["request"];
  }, BOOKLISTS_KEY);
  // Dispatching twice in the same task also probes the synchronous reentry lock.
  await page
    .getByTestId("booklist-editor")
    .locator("form")
    .evaluate((form) => {
      form.dispatchEvent(
        new Event("submit", { bubbles: true, cancelable: true }),
      );
      form.dispatchEvent(
        new Event("submit", { bubbles: true, cancelable: true }),
      );
    });
  await expect
    .poll(() => page.evaluate(() => window.booklistTestHooks?.lockRequests))
    .toBe(1);
  await expect(page.getByTestId("booklist-save")).toBeDisabled();
  await expect(page.getByTestId("booklist-name")).toBeDisabled();
  await expect(
    page.getByRole("button", { name: "关闭书单窗口", exact: true }),
  ).toBeDisabled();
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("booklist-editor")).toBeVisible();
  expect(await rawStorage(page, BOOKLISTS_KEY)).toBeNull();
  await page.evaluate(() => window.booklistTestHooks?.releaseWrite?.());
  await expect(page.getByTestId("booklist-editor")).toBeHidden();
  expect((await readBooklists(page)).lists).toHaveLength(1);
  expect(
    await page.evaluate(() => window.booklistTestHooks?.lockRequests),
  ).toBe(1);
});

test("a narrow picker scrolls long booklist names while keeping its save action reachable", async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 600 });
  await seedLists(
    page,
    Array.from({ length: 50 }, (_, index) =>
      list(
        "target-" + index,
        "书单 " + index + " · " + "很长的作品整理名称".repeat(5),
      ),
    ),
  );
  await openDiscovery(page);
  await page.getByTestId("open-rain").click();
  await page.getByTestId("detail-booklist").click();
  const picker = page.getByTestId("booklist-picker");
  await page.getByTestId("booklist-target-target-0").check();
  await page.getByTestId("booklist-target-target-49").check();
  expect(
    await picker
      .locator(".booklist-picker-options")
      .evaluate((element) => element.scrollTop),
  ).toBeGreaterThan(0);
  await expect(page.getByTestId("booklist-picker-save")).toBeInViewport();
  await expectNoOverflow(page);
  await page.getByTestId("booklist-picker-save").click();
  await expect(picker).toBeHidden();
  const document = await readBooklists(page);
  expect(document.lists[0].members).toEqual([reference("JM", "rain")]);
  expect(document.lists[49].members).toEqual([reference("JM", "rain")]);
  expect(
    document.lists.slice(1, 49).every((item) => item.members.length === 0),
  ).toBe(true);
});

test("modal rereads recover revision conflicts while retaining drafts, targets and external lists", async ({
  page,
}) => {
  await openBooklists(page);
  await page.getByTestId("booklist-create").click();
  await page.getByTestId("booklist-name").fill("冲突时保留的草稿");
  const external = list("external", "其他窗口的新书单", [
    reference("Pica", "unknown-external"),
  ]);
  const externalRaw = JSON.stringify({ version: 1, lists: [external] });
  await page.evaluate(({ key, raw }) => localStorage.setItem(key, raw), {
    key: BOOKLISTS_KEY,
    raw: externalRaw,
  });

  await page.getByTestId("booklist-save").click();
  const editor = page.getByTestId("booklist-editor");
  await expect(editor.getByRole("alert")).toContainText("草稿和选择已保留");
  expect(await rawStorage(page, BOOKLISTS_KEY)).toBe(externalRaw);
  await page.getByTestId("booklist-editor-reload").click();
  await expect(page.getByTestId("booklists-error")).toBeHidden();
  await expect(page.getByTestId("booklist-save")).toBeEnabled();
  await expect(editor).toBeVisible();
  await expect(page.getByTestId("booklist-name")).toHaveValue(
    "冲突时保留的草稿",
  );
  expect(await rawStorage(page, BOOKLISTS_KEY)).toBe(externalRaw);
  await page.getByTestId("booklist-save").click();
  await expect(editor).toBeHidden();
  const afterCreate = await readBooklists(page);
  expect(afterCreate.lists).toHaveLength(2);
  expect(afterCreate.lists[0]).toEqual(external);
  const created = afterCreate.lists[1];
  expect(created.name).toBe("冲突时保留的草稿");

  await openDiscovery(page);
  await page.getByTestId("open-rain").click();
  await page.getByTestId("detail-booklist").click();
  await page.getByTestId("booklist-target-external").check();
  await page.getByTestId("booklist-target-" + created.id).check();
  await page.getByTestId("booklist-picker-create").click();
  await page.getByTestId("booklist-picker-name").fill("未提交的新书单草稿");
  await page.getByTestId("booklist-picker-existing").click();
  const newest = list("external-new", "又一个外部书单", [
    reference("JM", "another-unknown"),
  ]);
  const latestRaw = JSON.stringify({
    version: 1,
    lists: [...afterCreate.lists, newest],
  });
  await page.evaluate(({ key, raw }) => localStorage.setItem(key, raw), {
    key: BOOKLISTS_KEY,
    raw: latestRaw,
  });
  await page.getByTestId("booklist-picker-save").click();
  const picker = page.getByTestId("booklist-picker");
  await expect(picker.getByRole("alert")).toContainText("草稿和选择已保留");
  expect(await rawStorage(page, BOOKLISTS_KEY)).toBe(latestRaw);
  await page.getByTestId("booklist-picker-reload").click();
  await expect(page.getByTestId("booklists-error")).toBeHidden();
  await expect(page.getByTestId("booklist-picker-save")).toBeEnabled();
  await expect(picker).toBeVisible();
  await expect(page.getByTestId("booklist-target-external")).toBeChecked();
  await expect(page.getByTestId("booklist-target-" + created.id)).toBeChecked();
  await expect(
    page.getByTestId("booklist-target-external-new"),
  ).not.toBeChecked();
  await page.getByTestId("booklist-picker-create").click();
  await expect(page.getByTestId("booklist-picker-name")).toHaveValue(
    "未提交的新书单草稿",
  );
  await page.getByTestId("booklist-picker-existing").click();
  expect(await rawStorage(page, BOOKLISTS_KEY)).toBe(latestRaw);
  await page.getByTestId("booklist-picker-save").click();
  await expect(picker).toBeHidden();
  const merged = await readBooklists(page);
  expect(merged.lists).toHaveLength(3);
  expect(merged.lists.find((item) => item.id === external.id)?.members).toEqual(
    [...external.members, reference("JM", "rain")],
  );
  expect(merged.lists.find((item) => item.id === created.id)?.members).toEqual([
    reference("JM", "rain"),
  ]);
  expect(merged.lists.find((item) => item.id === newest.id)).toEqual(newest);
});
