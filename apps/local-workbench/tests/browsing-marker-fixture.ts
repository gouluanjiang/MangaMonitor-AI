import type { Page } from "@playwright/test";
import type { AccountSummary } from "../src/source-types.ts";
import type { BrowsingBaseline } from "../src/browsing-markers.ts";

declare global {
  interface Window {
    syntheticBrowsingMarkers: {
      failure: string | null;
      call(
        command: string,
        args: Record<string, unknown>,
        accounts: AccountSummary[],
      ): unknown;
      seed(
        source: string,
        accountId: string,
        surface: string,
        baseline: BrowsingBaseline | null,
      ): void;
    };
  }
}

/** Fixture-only localStorage models the private native sidecar across page reloads. */
export async function installBrowsingMarkerFixture(page: Page) {
  await page.addInitScript(() => {
    const key = (source: unknown, accountId: unknown, surface: unknown) =>
      "synthetic-browsing-markers:" +
      JSON.stringify([source, accountId, surface]);
    window.syntheticBrowsingMarkers = {
      failure: null,
      seed(source, accountId, surface, baseline) {
        localStorage.setItem(
          key(source, accountId, surface),
          JSON.stringify({ revision: 1, value: { version: 1, baseline } }),
        );
      },
      call(command, args, accounts) {
        if (
          command !== "browsing_markers_read" &&
          command !== "browsing_markers_write"
        )
          return undefined;
        if (this.failure) throw { code: this.failure };
        const account = accounts.find(
          (value) =>
            value.source === args.source &&
            value.sessionId === args.sessionId &&
            value.state === "connected",
        );
        if (!account?.accountId) throw { code: "SESSION_CHANGED" };
        if (args.surface !== "recent" && args.surface !== "authors")
          throw { code: "BROWSING_BASELINE_INVALID" };
        const storageKey = key(account.source, account.accountId, args.surface);
        const previous = JSON.parse(
          localStorage.getItem(storageKey) ??
            '{"revision":0,"value":{"version":1,"baseline":null}}',
        );
        if (command === "browsing_markers_read") return previous;
        if (previous.value.version !== 1) throw { code: "UNSUPPORTED_SCHEMA" };
        const next = {
          revision: previous.revision + 1,
          value: { version: 1, baseline: structuredClone(args.baseline) },
        };
        localStorage.setItem(storageKey, JSON.stringify(next));
        return next;
      },
    };
  });
}
