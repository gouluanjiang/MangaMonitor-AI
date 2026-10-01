import { expect, test } from "@playwright/test";
import { installWorkflow } from "./workflow-fixture.ts";

test("special follows keep baseline, unread and ordinary follows separate across browsing", async ({
  page,
}) => {
  await installWorkflow(page);
  await page.evaluate(() => {
    // The shared workflow starts with one old omission, not two new updates.
    // Finish its synthetic check before selecting this test's two update rows.
    window.workflowTest.finishCheck();
    const previous = window.__TAURI_INTERNALS__!.invoke;
    const hooks = {
      scopes: window.workflowTest.accounts.map((account) => ({
        source: account.source,
        sessionId: account.sessionId,
      })),
      authors: [
        {
          author: "合成关注作者",
          enabled: true,
          baselinesComplete: 1,
          errorCodes: ["SOURCE_TIMEOUT"],
        },
      ],
      updates: window.workflowTest.discovery.records
        .slice(0, 2)
        .map((record) => ({
          work: record.work,
          authors: ["合成关注作者"],
          discoveredAt: 100,
          readAt: null as number | null,
        })),
      run: {
        id: 1,
        phase: "partial",
        startedAt: 1,
        finishedAt: 100,
        newCount: 2,
        errorCode: null,
      },
      marks: 0,
      sets: [] as boolean[],
      fail: false,
    };
    Object.assign(window, { specialTest: hooks });
    window.__TAURI_INTERNALS__!.invoke = async (command, args = {}) => {
      if (command.startsWith("special_")) {
        if (hooks.fail) throw { code: "STORE_UNAVAILABLE" };
        if (command === "special_mark_read") {
          hooks.marks++;
          const identity = args.identity as {
            source: string;
            workId: string;
          } | null;
          hooks.updates.forEach((row) => {
            if (
              !identity ||
              (identity.source === row.work.source &&
                identity.workId === row.work.workId)
            )
              row.readAt = 200;
          });
        }
        if (command === "special_set") {
          hooks.sets.push(Boolean(args.enabled));
          hooks.authors[0].enabled = Boolean(args.enabled);
        }
        if (command === "special_progress") return structuredClone(hooks.run);
        return structuredClone({
          ...hooks,
          updates: hooks.authors[0].enabled ? hooks.updates : [],
        });
      }
      return previous(command, args);
    };
  });
  await page.getByTestId("nav-special").click();
  await expect(page.getByTestId("special-counts")).toContainText(
    "未读作品 2 部",
  );
  await expect(
    page.getByText("部分作者基线尚未完成，旧作暂不计为新更新"),
  ).toBeVisible();
  await page.getByTestId("nav-queue").click();
  await page.getByTestId("nav-special").click();
  await expect(page.getByTestId("special-counts")).toContainText(
    "未读作品 2 部",
  );
  const first = page.getByTestId("special-panel").locator("article").first();
  await first.locator("h3 button").click();
  await expect(page.getByTestId("source-detail-back")).toBeVisible();
  await page.getByTestId("source-detail-back").click();
  await expect(page.getByTestId("special-counts")).toContainText(
    "未读作品 1 部",
  );
  await page.evaluate(() => {
    (window as any).specialTest.fail = true;
  });
  await page.getByRole("button", { name: "刷新状态", exact: true }).click();
  await expect(
    page.getByTestId("special-panel").getByRole("alert"),
  ).toContainText("已有记录保留");
  await expect(page.getByTestId("special-counts")).toContainText(
    "未读作品 1 部",
  );
  await page.evaluate(() => {
    (window as any).specialTest.fail = false;
  });
  await page.getByRole("button", { name: "全部标为已读", exact: true }).click();
  await expect(page.getByTestId("special-counts")).toContainText(
    "未读作品 0 部",
  );
  await page.getByTestId("nav-authors").click();
  await page.getByRole("button", { name: "取消特别关注", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "设为特别关注", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "取消关注", exact: true }),
  ).toBeVisible();
  expect(
    await page.evaluate(() =>
      window.workflowTest.calls.filter(
        (call) => call.command === "source_follow",
      ),
    ),
  ).toHaveLength(0);
});
