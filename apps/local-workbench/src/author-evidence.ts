import type { DiscoverySnapshot } from "./completion-types.ts";
import type { Source } from "./source-types.ts";

type DiscoveryRecord = DiscoverySnapshot["records"][number];

const normalize = (name: string) =>
  name.normalize("NFKC").toLowerCase().replace(/\s+/gu, " ").trim();

/** Explicit name components only; never fold kana/voicing or match substrings. */
function nameParts(name: string): { names: Set<string>; members: Set<string> } {
  const names = new Set<string>();
  const members = new Set<string>();
  const value = normalize(name);
  const add = (part: string, nested: boolean) => {
    const clean = part.trim();
    if (!clean || /\.{2,}|…/u.test(clean)) return;
    names.add(clean);
    if (nested) members.add(clean);
  };
  add(value, false);
  // Malformed/truncated group labels are kept as other keyword results.
  // Do not extract a seemingly valid name from an unclosed group.
  const paired = value
    .replace(/[\[【「『]/gu, "(")
    .replace(/[\]】」』]/gu, ")");
  let depth = 0;
  for (const char of paired) {
    if (char === "(") depth++;
    if (char === ")" && --depth < 0) return { names, members };
  }
  if (depth) return { names, members };
  function collect(text: string, nested: boolean) {
    add(text, nested);
    let part = "",
      level = 0,
      group = "";
    const addList = (value: string) => {
      for (const item of value.split(/[、,;]|\s+[&×/]\s+/u)) add(item, nested);
    };
    for (const char of text) {
      if (char === "(") {
        if (level++ === 0) {
          addList(part);
          part = "";
          group = "";
        } else group += char;
      } else if (char === ")") {
        if (--level === 0) collect(group, true);
        else group += char;
      } else if (level) group += char;
      else part += char;
    }
    addList(part);
  }
  collect(paired, false);
  return { names, members };
}

export function authorNameMatches(query: string, sourceName: string): boolean {
  const expected = normalize(query);
  if (!expected) return false;
  const candidate = nameParts(sourceName);
  if (candidate.names.has(expected)) return true;
  // A followed "Circle (Writer)" may be returned as just "Writer".
  // Sharing only the outer circle label does not establish that writer.
  return [...nameParts(query).members].some((member) =>
    candidate.names.has(member),
  );
}

export function partitionAuthorRecords(
  records: DiscoveryRecord[],
  author = "",
  source: Source | "all" = "all",
): { confirmed: DiscoveryRecord[]; other: DiscoveryRecord[] } {
  const confirmed: DiscoveryRecord[] = [],
    other: DiscoveryRecord[] = [];
  for (const record of records) {
    if (source !== "all" && record.work.source !== source) continue;
    const queries = record.matchedAuthors.filter(
      (name) => !author || name === author,
    );
    if (!queries.length) continue;
    // Query membership and the legacy authorVerified flag are not authorship.
    // Derive from current metadata even for results saved by older versions.
    const matches = queries.some((query) =>
      record.work.authors.some((name) => authorNameMatches(query, name)),
    );
    (matches ? confirmed : other).push(record);
  }
  return { confirmed, other };
}
