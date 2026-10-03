const transient = new Set([
  "SOURCE_TIMEOUT",
  "SOURCE_CONNECTION_FAILED",
  "SOURCE_REQUEST_FAILED",
  "SOURCE_RATE_LIMITED",
  "SOURCE_COVER_RATE_LIMITED",
  "SOURCE_COVER_SERVER_ERROR",
]);

export const automaticCoverRetry = (code: string) => transient.has(code);

/** Only a validated relative delay crosses IPC; never retain response headers. */
export function coverRetryAfter(error: unknown): number {
  const value =
    typeof error === "object" && error !== null && "retryAfterMs" in error
      ? error.retryAfterMs
      : undefined;
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0
    ? Math.min(value, Number.MAX_SAFE_INTEGER - Date.now())
    : 0;
}

export function coverBackoff(failure: number, random = Math.random): number {
  return (
    Math.min(30000, 1000 * 2 ** (failure - 1)) + Math.floor(random() * 300)
  );
}

/** Waiting retries do not occupy a network slot; leaving the viewport stops waiting. */
export function waitForCover(
  delay: number,
  signal: AbortSignal,
): Promise<void> {
  return new Promise((resolve) => {
    if (signal.aborted) {
      resolve();
      return;
    }
    const timer = setTimeout(finish, Math.min(delay, 2147483647));
    function finish() {
      clearTimeout(timer);
      signal.removeEventListener("abort", finish);
      resolve();
    }
    signal.addEventListener("abort", finish, { once: true });
  });
}
