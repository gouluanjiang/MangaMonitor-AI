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

test("detail special follow first adds ordinary follow and cancellation keeps it", async ({
  page,
}) => {
  await installWorkflow(page);
  await page.evaluate(() => {
    const previous = window.__TAURI_INTERNALS__!.invoke;
    let followed = false,
      enabled = false;
    const mutationCalls: { command: string; args: Record<string, unknown> }[] =
      [];
    Object.assign(window, { detailSpecialCalls: mutationCalls });
    window.__TAURI_INTERNALS__!.invoke = async (command, args = {}) => {
      const scopes = window.workflowTest.accounts.map((a) => ({
        source: a.source,
        sessionId: a.sessionId,
      }));
      const special = () => ({
        scopes,
        authors: enabled
          ? [
              {
                author: "合成新作者",
                enabled: true,
                baselinesComplete: 0,
                errorCodes: [],
              },
            ]
          : [],
        updates: [],
        run: {
          id: 0,
          phase: "idle",
          startedAt: null,
          finishedAt: null,
          newCount: 0,
          errorCode: null,
        },
      });
      if (command === "source_following")
        return {
          source: args.source,
          sessionId: args.sessionId,
          revision: followed ? 2 : 1,
          authors: followed ? ["合成关注作者", "合成新作者"] : ["合成关注作者"],
          works: [],
        };
      if (command === "source_follow") {
        mutationCalls.push({ command, args });
        followed = Boolean(args.desired);
        return {
          source: args.source,
          sessionId: args.sessionId,
          revision: 2,
          authors: ["合成关注作者", "合成新作者"],
          works: [],
        };
      }
      if (command === "special_set") {
        mutationCalls.push({ command, args });
        if (!followed) throw { code: "DISCOVERY_AUTHOR_NOT_FOLLOWED" };
        enabled = Boolean(args.enabled);
        return special();
      }
      if (command.startsWith("special_"))
        return command === "special_progress" ? special().run : special();
      return previous(command, args);
    };
  });
  await page.getByTestId("nav-completion").click();
  await page
    .getByTestId("completion-panel")
    .locator("article h3 button")
    .first()
    .click();
  await expect(page.getByTestId("source-detail")).toBeVisible();
  const author = page
    .getByTestId("source-follow-author-合成新作者")
    .locator("..");
  await author
    .getByRole("button", { name: "设为特别关注", exact: true })
    .click();
  await expect(
    author.getByRole("button", { name: "取消特别关注", exact: true }),
  ).toBeVisible();
  await expect(
    author.getByRole("button", { name: "取消作者关注", exact: true }),
  ).toBeVisible();
  await author
    .getByRole("button", { name: "取消特别关注", exact: true })
    .click();
  await expect(
    author.getByRole("button", { name: "设为特别关注", exact: true }),
  ).toBeVisible();
  await expect(
    author.getByRole("button", { name: "取消作者关注", exact: true }),
  ).toBeVisible();
  const calls = await page.evaluate(() => (window as any).detailSpecialCalls);
  expect(calls.map((call: any) => call.command)).toEqual([
    "source_follow",
    "special_set",
    "special_set",
  ]);
  expect(calls[0].args.desired).toBe(true);
  expect(calls[1].args.enabled).toBe(true);
  expect(calls[2].args.enabled).toBe(false);
});
