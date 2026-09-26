import type { DiscoverySnapshot } from "./completion-types.ts";
import type {
  AuthorQueryPolicy,
  AuthorWorkCredit,
  Source,
  SourceWork,
} from "./source-types.ts";

type DiscoveryRecord = DiscoverySnapshot["records"][number];

const normalize = (name: string) =>
  name.normalize("NFKC").toLowerCase().replace(/\s+/gu, " ").trim();

type CreditWork = Pick<SourceWork, "authors"> &
  Partial<Pick<SourceWork, "source" | "workId" | "authorCreditReview">>;
const creditSet = (authors: string[]) =>
  JSON.stringify([...new Set(authors.map(normalize))].sort());
const expectedCreditSets = (rule: AuthorWorkCredit) =>
  [rule.expectedAuthors, ...(rule.expectedAuthorVariants ?? [])]
    .map(creditSet)
    .sort();

/** Compile once for large saved catalogs; a rule never becomes a name alias. */
function creditProjector(policies: AuthorQueryPolicy[]) {
  const rules = new Map<string, AuthorWorkCredit | null>();
  for (const policy of policies)
    for (const rule of policy.workCredits ?? []) {
      const key = JSON.stringify([policy.source, rule.workId]);
      const existing = rules.get(key);
      if (existing === null) continue;
      if (
        existing &&
        (JSON.stringify(expectedCreditSets(existing)) !==
          JSON.stringify(expectedCreditSets(rule)) ||
          creditSet(existing.correctedAuthors) !==
            creditSet(rule.correctedAuthors))
      ) {
        rules.set(key, null);
      } else rules.set(key, rule);
    }
  return <T extends CreditWork>(work: T): T => {
    const originalAuthors =
      work.authorCreditReview?.originalAuthors ?? work.authors;
    const rule = rules.get(JSON.stringify([work.source, work.workId]));
    if (
      !rule ||
      !expectedCreditSets(rule).includes(creditSet(originalAuthors))
    ) {
      if (!work.authorCreditReview) return work;
      const { authorCreditReview: _review, ...raw } = work;
      return { ...raw, authors: [...originalAuthors] } as T;
    }
    return {
      ...work,
      authors: [...rule.correctedAuthors],
      authorCreditReview: { originalAuthors: [...originalAuthors] },
    };
  };
}

export function projectAuthorWork<T extends CreditWork>(
  work: T,
  policies: AuthorQueryPolicy[] = [],
): T {
  return creditProjector(policies)(work);
}

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

export function workHasAuthor(
  work: CreditWork,
  query: string,
  policy?: AuthorQueryPolicy,
): boolean {
  const applicable =
    policy?.author === query && policy.source === work.source
      ? policy
      : undefined;
  return authorsMatch(
    projectAuthorWork(work, applicable ? [applicable] : []).authors,
    query,
    applicable,
  );
}

function authorsMatch(
  authors: string[],
  query: string,
  applicable?: AuthorQueryPolicy,
): boolean {
  const names = [query, ...(applicable?.verifiedAliases ?? [])];
  const credits = new Set((applicable?.exactCredits ?? []).map(normalize));
  return authors.some(
    (name) =>
      names.some((expected) => authorNameMatches(expected, name)) ||
      credits.has(normalize(name)),
  );
}

export function partitionAuthorWorks(
  works: SourceWork[],
  query: string,
  policy?: AuthorQueryPolicy,
): { confirmed: SourceWork[]; other: SourceWork[] } {
  const confirmed: SourceWork[] = [],
    other: SourceWork[] = [];
  const applicable = policy?.author === query ? policy : undefined;
  const project = creditProjector(applicable ? [applicable] : []);
  for (const raw of works) {
    const work = project(raw);
    (authorsMatch(
      work.authors,
      query,
      applicable?.source === work.source ? applicable : undefined,
    )
      ? confirmed
      : other
    ).push(work);
  }
  return { confirmed, other };
}

export function partitionAuthorRecords(
  records: DiscoveryRecord[],
  author = "",
  source: Source | "all" = "all",
  policies: AuthorQueryPolicy[] = [],
): { confirmed: DiscoveryRecord[]; other: DiscoveryRecord[] } {
  const confirmed: DiscoveryRecord[] = [],
    other: DiscoveryRecord[] = [];
  const policiesByAuthor = new Map(
    policies.map((policy) => [
      JSON.stringify([policy.source, policy.author]),
      policy,
    ]),
  );
  const project = creditProjector(policies);
  const policiesBySource = new Map<Source, AuthorQueryPolicy[]>();
  for (const policy of policies) {
    const entries = policiesBySource.get(policy.source) ?? [];
    entries.push(policy);
    policiesBySource.set(policy.source, entries);
  }
  for (const record of records) {
    if (source !== "all" && record.work.source !== source) continue;
    const work = project(record.work);
    const memberships = new Set(record.matchedAuthors);
    // A reviewed work already saved under a wrong query can appear for the
    // actual followed author. This is a view only, not search coverage evidence.
    if (work.authorCreditReview)
      for (const policy of policiesBySource.get(work.source) ?? [])
        if (authorsMatch(work.authors, policy.author, policy))
          memberships.add(policy.author);
    const queries = [...memberships].filter(
      (name) => !author || name === author,
    );
    if (!queries.length) continue;
    // Query membership and the legacy authorVerified flag are not authorship.
    // Derive from current metadata even for results saved by older versions.
    const matches = queries.some((query) =>
      authorsMatch(
        work.authors,
        query,
        policiesByAuthor.get(JSON.stringify([record.work.source, query])),
      ),
    );
    (matches ? confirmed : other).push(
      work === record.work ? record : { ...record, work },
    );
  }
  return { confirmed, other };
}
