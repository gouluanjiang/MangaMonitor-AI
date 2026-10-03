import type { DiscoverySnapshot } from "./completion-types.ts";
import { authorQueryError } from "./author-query.ts";
import { isOutsideJmAuthorScope } from "./content-filter.ts";
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
  Partial<
    Pick<
      SourceWork,
      "source" | "workId" | "authorCreditReview" | "tags" | "categories"
    >
  >;
const excludedAuthorScope = (work: CreditWork) =>
  work.source !== undefined &&
  isOutsideJmAuthorScope({
    ...work,
    source: work.source,
    workId: work.workId ?? "",
  });
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
  if (
    excludedAuthorScope(work) ||
    authorQueryError(query) === "AUTHOR_QUERY_PLACEHOLDER"
  )
    return false;
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
    if (isOutsideJmAuthorScope(raw)) continue;
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
  followedAuthors?: string[],
): { confirmed: DiscoveryRecord[]; other: DiscoveryRecord[] } {
  const confirmed: DiscoveryRecord[] = [],
    other: DiscoveryRecord[] = [];
  // The caller supplies the current followed set, not the queries that happened
  // to discover each work. Legacy callers still get an explicit local universe.
  const names = followedAuthors ?? [
    ...new Set([
      ...records.flatMap((record) => record.matchedAuthors),
      ...policies.map((policy) => policy.author),
      ...(author ? [author] : []),
    ]),
  ];
  const followed = new Set(names);
  const classify = createAuthorMembershipProjector(names, policies);
  for (const record of records) {
    if (source !== "all" && record.work.source !== source) continue;
    if (isOutsideJmAuthorScope(record.work)) continue;
    const { work, authors } = classify(record.work);
    const matches = author ? authors.includes(author) : authors.length > 0;
    const foundBySelectedQuery = record.matchedAuthors.some(
      (name) => followed.has(name) && (!author || name === author),
    );
    if (!matches && !foundBySelectedQuery) continue;
    (matches ? confirmed : other).push(
      work === record.work ? record : { ...record, work },
    );
  }
  return { confirmed, other };
}

/** Compile exact signature tokens once, rather than comparing every work with
 * every followed name. Discovery query membership is intentionally irrelevant. */
export function createAuthorMembershipProjector(
  followedAuthors: string[],
  policies: AuthorQueryPolicy[] = [],
) {
  const project = creditProjector(policies);
  const byPolicy = new Map(
    policies.map((policy) => [
      JSON.stringify([policy.source, policy.author]),
      policy,
    ]),
  );
  const tokenIndex = new Map<Source, Map<string, Set<string>>>();
  const exactIndex = new Map<Source, Map<string, Set<string>>>();
  const add = (
    index: Map<string, Set<string>>,
    token: string,
    author: string,
  ) => {
    if (!token) return;
    const authors = index.get(token) ?? new Set<string>();
    authors.add(author);
    index.set(token, authors);
  };
  for (const source of ["JM", "Pica"] as const) {
    const tokens = new Map<string, Set<string>>();
    const exact = new Map<string, Set<string>>();
    for (const author of new Set(followedAuthors)) {
      if (authorQueryError(author) === "AUTHOR_QUERY_PLACEHOLDER") continue;
      const policy = byPolicy.get(JSON.stringify([source, author]));
      for (const name of [author, ...(policy?.verifiedAliases ?? [])]) {
        add(tokens, normalize(name), author);
        for (const member of nameParts(name).members)
          add(tokens, member, author);
      }
      for (const credit of policy?.exactCredits ?? [])
        add(exact, normalize(credit), author);
    }
    tokenIndex.set(source, tokens);
    exactIndex.set(source, exact);
  }
  return <T extends CreditWork>(raw: T): { work: T; authors: string[] } => {
    const work = project(raw),
      authors = new Set<string>();
    if (!work.source || excludedAuthorScope(work)) return { work, authors: [] };
    for (const credit of work.authors) {
      for (const token of nameParts(credit).names)
        for (const author of tokenIndex.get(work.source)?.get(token) ?? [])
          authors.add(author);
      for (const author of exactIndex
        .get(work.source)
        ?.get(normalize(credit)) ?? [])
        authors.add(author);
    }
    return { work, authors: [...authors] };
  };
}
