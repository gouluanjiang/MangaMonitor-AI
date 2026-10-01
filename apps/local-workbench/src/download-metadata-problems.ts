import type { DownloadSource } from "./download-types.ts";

// Exact allowlist shared by inline errors and the redacted diagnostic summary.
// No source field value, work ID, account, URL or local path is retained here.
export const downloadMetadataMessages = {
  DOWNLOAD_METADATA_INVALID: "作品信息未通过下载校验，尚未取得具体原因。",
  DOWNLOAD_METADATA_MISSING: "来源详情未返回作品信息，未加入下载队列。",
  DOWNLOAD_METADATA_IDENTITY_MISMATCH:
    "来源详情的作品编号或来源不一致，未加入下载队列。",
  DOWNLOAD_METADATA_ID_INVALID: "来源作品编号格式不符合下载要求。",
  DOWNLOAD_METADATA_TITLE_EMPTY: "来源作品标题为空，未通过下载校验。",
  DOWNLOAD_METADATA_TITLE_TOO_LONG: "来源作品标题过长，未通过下载校验。",
  DOWNLOAD_METADATA_TITLE_CONTROL:
    "来源作品标题含不支持的控制字符，未通过下载校验。",
  DOWNLOAD_METADATA_AUTHORS_TOO_MANY:
    "来源返回的作者条目过多，未通过下载校验。",
  DOWNLOAD_METADATA_AUTHOR_EMPTY: "来源作者字段含空条目，未通过下载校验。",
  DOWNLOAD_METADATA_AUTHOR_TOO_LONG: "来源作者字段过长，未通过下载校验。",
  DOWNLOAD_METADATA_AUTHOR_CONTROL:
    "来源作者字段含不支持的控制字符，未通过下载校验。",
  DOWNLOAD_METADATA_TAGS_TOO_MANY: "来源返回的标签条目过多，未通过下载校验。",
  DOWNLOAD_METADATA_TAG_EMPTY: "来源标签字段含空条目，未通过下载校验。",
  DOWNLOAD_METADATA_TAG_TOO_LONG: "来源标签字段过长，未通过下载校验。",
  DOWNLOAD_METADATA_TAG_CONTROL:
    "来源标签字段含不支持的控制字符，未通过下载校验。",
  DOWNLOAD_METADATA_DATE_INVALID: "来源更新时间格式不符合下载要求。",
  DOWNLOAD_METADATA_DESCRIPTION_TOO_LONG: "来源简介过长，未通过下载校验。",
  DOWNLOAD_METADATA_DESCRIPTION_CONTROL:
    "来源简介含不支持的控制字符，未通过下载校验。",
} as const;

export type DownloadMetadataCode = keyof typeof downloadMetadataMessages;
export interface DownloadMetadataProblem {
  source: DownloadSource;
  code: DownloadMetadataCode;
  occurredAt: number;
}
export function downloadMetadataCode(
  cause: unknown,
): DownloadMetadataCode | null {
  const code =
    typeof cause === "string"
      ? cause
      : cause && typeof cause === "object" && "code" in cause
        ? cause.code
        : null;
  return typeof code === "string" &&
    Object.hasOwn(downloadMetadataMessages, code)
    ? (code as DownloadMetadataCode)
    : null;
}
export function retainDownloadMetadataProblems(
  retained: readonly DownloadMetadataProblem[],
  source: DownloadSource,
  causes: readonly unknown[],
  occurredAt = Date.now(),
): DownloadMetadataProblem[] {
  const incoming = causes.flatMap((cause) => {
    const code = downloadMetadataCode(cause);
    return code ? [{ source, code, occurredAt }] : [];
  });
  return [...incoming.reverse(), ...retained].slice(0, 20);
}
