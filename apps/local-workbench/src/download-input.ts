import type { DownloadSource } from "./download-types.ts";

// Mirrors the existing desktop parse_work_id allowlist only to locate an
// already queued task. New downloads still pass the original input through the
// native validator; no pasted URL is requested by the frontend.
const jmApiHosts = new Set([
  "www.cdnhth.cc",
  "www.cdnzack.cc",
  "www.cdnhth.net",
  "www.cdnbea.net",
  "www.cdn-mspjmapiproxy.xyz",
]);
function normalizedId(source: DownloadSource, input: string): string | null {
  if (source === "Pica")
    return /^[a-f0-9]{24}$/i.test(input) ? input.toLowerCase() : null;
  if (!/^\d{1,19}$/.test(input) || !/[1-9]/.test(input)) return null;
  return input.replace(/^0+/, "");
}
export function downloadInputWorkId(
  source: DownloadSource,
  input: string,
): string | null {
  const value = input.trim();
  if (value.length > 2048) return null;
  const direct = normalizedId(
    source,
    source === "JM" ? value.replace(/^JM(?=\d+$)/i, "") : value,
  );
  if (direct) return direct;
  try {
    const url = new URL(value);
    if (
      url.protocol !== "https:" ||
      url.username ||
      url.password ||
      url.port ||
      url.hash ||
      value.includes("#")
    )
      return null;
    const parts = url.pathname.replace(/^\/+|\/+$/g, "").split("/");
    if (source === "Pica")
      return url.hostname === "picaapi.picacomic.com" &&
        !value.includes("?") &&
        parts.length === 2 &&
        parts[0] === "comics"
        ? normalizedId(source, parts[1])
        : null;
    if (
      !jmApiHosts.has(url.hostname) &&
      !["18comic.vip", "www.18comic.vip"].includes(url.hostname)
    )
      return null;
    if (!value.includes("?") && parts.length === 2 && parts[0] === "album")
      return normalizedId(source, parts[1]);
    const pairs = [...url.searchParams];
    return jmApiHosts.has(url.hostname) &&
      url.pathname === "/album" &&
      pairs.length === 1 &&
      pairs[0][0] === "id"
      ? normalizedId(source, pairs[0][1])
      : null;
  } catch {
    return null;
  }
}

export const downloadSubmissionKey = (source: DownloadSource, input: string) =>
  `${source}:${downloadInputWorkId(source, input) ?? input.trim()}`;
