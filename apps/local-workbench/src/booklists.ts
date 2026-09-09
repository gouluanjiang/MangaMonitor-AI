export interface WorkReference {
  source: "JM" | "Pica";
  workId: string;
}

export interface Booklist {
  id: string;
  name: string;
  createdAt: number;
  updatedAt: number;
  archived: boolean;
  members: WorkReference[];
}

export interface BooklistsDocument {
  version: 1;
  lists: Booklist[];
}

export const BOOKLIST_LIMITS = {
  lists: 100,
  membersPerList: 2000,
  totalMembers: 20000,
  nameCharacters: 80,
} as const;

const listIdPattern = /^[A-Za-z0-9_-]{1,80}$/;
const workIdPattern = /^[A-Za-z0-9_-]{1,160}$/;

function objectWithKeys(
  value: unknown,
  keys: readonly string[],
): value is Record<string, unknown> {
  return (
    typeof value === "object" &&
    value !== null &&
    !Array.isArray(value) &&
    Object.keys(value).length === keys.length &&
    keys.every((key) => Object.prototype.hasOwnProperty.call(value, key))
  );
}

function validateTime(value: unknown): asserts value is number {
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 0) {
    throw new Error("书单时间无效，请检查本地数据后重试。");
  }
}

function normalizedName(value: unknown): string {
  if (typeof value !== "string") throw new Error("请输入书单名称。");
  const name = value.trim();
  if (/\p{Cc}/u.test(name)) {
    throw new Error("书单名称不能包含控制字符或换行。");
  }
  if (!name) throw new Error("书单名称不能为空。");
  if (Array.from(name).length > BOOKLIST_LIMITS.nameCharacters) {
    throw new Error("书单名称最多为 80 个字符。");
  }
  return name;
}

function validateReference(value: unknown): asserts value is WorkReference {
  if (
    !objectWithKeys(value, ["source", "workId"]) ||
    (value.source !== "JM" && value.source !== "Pica") ||
    typeof value.workId !== "string" ||
    !workIdPattern.test(value.workId)
  ) {
    throw new Error("作品引用无效：需要明确的来源和有效作品编号。");
  }
}

function referenceKey(reference: WorkReference): string {
  return reference.source + ":" + reference.workId;
}

function checkedReferences(
  references: readonly WorkReference[],
): WorkReference[] {
  if (!Array.isArray(references)) {
    throw new Error("作品列表格式无效，请重新选择作品。");
  }
  const unique = new Map<string, WorkReference>();
  for (const reference of references) {
    validateReference(reference);
    unique.set(referenceKey(reference), {
      source: reference.source,
      workId: reference.workId,
    });
  }
  return [...unique.values()];
}

export function initialBooklists(): BooklistsDocument {
  return { version: 1, lists: [] };
}

// Validation never repairs or discards data. The storage layer decides how to
// preserve an unreadable document and surface a retryable read error.
export function validateBooklists(value: unknown): BooklistsDocument {
  if (
    !objectWithKeys(value, ["version", "lists"]) ||
    value.version !== 1 ||
    !Array.isArray(value.lists)
  ) {
    throw new Error("书单数据格式或版本不受支持，原有数据应保留。");
  }
  if (value.lists.length > BOOKLIST_LIMITS.lists) {
    throw new Error("最多保留 100 个书单，包含已归档书单。");
  }
  const ids = new Set<string>();
  const activeNames = new Set<string>();
  let totalMembers = 0;
  for (const list of value.lists) {
    if (
      !objectWithKeys(list, [
        "id",
        "name",
        "createdAt",
        "updatedAt",
        "archived",
        "members",
      ]) ||
      typeof list.id !== "string" ||
      !listIdPattern.test(list.id) ||
      typeof list.archived !== "boolean" ||
      !Array.isArray(list.members)
    ) {
      throw new Error("书单内容格式无效，原有数据应保留。");
    }
    if (ids.has(list.id)) throw new Error("书单编号重复，原有数据应保留。");
    ids.add(list.id);
    const name = normalizedName(list.name);
    if (name !== list.name) {
      throw new Error("已保存的书单名称含有首尾空白，请检查本地数据。");
    }
    if (!list.archived) {
      if (activeNames.has(name))
        throw new Error("已有同名书单，请使用其他名称。");
      activeNames.add(name);
    }
    validateTime(list.createdAt);
    validateTime(list.updatedAt);
    if (list.updatedAt < list.createdAt) {
      throw new Error("书单更新时间不能早于创建时间。");
    }
    if (list.members.length > BOOKLIST_LIMITS.membersPerList) {
      throw new Error("每个书单最多保留 2000 部作品。");
    }
    totalMembers += list.members.length;
    if (totalMembers > BOOKLIST_LIMITS.totalMembers) {
      throw new Error("所有书单合计最多保留 20000 个作品关联。");
    }
    const members = new Set<string>();
    for (const reference of list.members) {
      validateReference(reference);
      const key = referenceKey(reference);
      if (members.has(key)) {
        throw new Error("书单内有重复作品关联，原有数据应保留。");
      }
      members.add(key);
    }
  }
  return value as unknown as BooklistsDocument;
}

function changeList(
  document: BooklistsDocument,
  id: string,
  now: number,
  change: (list: Booklist, timestamp: number) => Booklist,
): BooklistsDocument {
  validateBooklists(document);
  validateTime(now);
  const index = document.lists.findIndex((list) => list.id === id);
  if (index < 0) throw new Error("这个书单已不可用，请刷新后重试。");
  const previous = document.lists[index];
  // A local clock moving backwards must not undo an existing edit timestamp.
  const next = change(previous, Math.max(now, previous.updatedAt));
  if (next === previous) return document;
  const lists = document.lists.map((list, position) =>
    position === index ? next : list,
  );
  return validateBooklists({ version: 1, lists });
}

export function createBooklist(
  document: BooklistsDocument,
  input: { id: string; name: string; now: number },
): BooklistsDocument {
  validateBooklists(document);
  if (!objectWithKeys(input, ["id", "name", "now"])) {
    throw new Error("新书单信息格式无效，请检查后重试。");
  }
  if (typeof input.id !== "string" || !listIdPattern.test(input.id)) {
    throw new Error("新书单编号无效，请重试。");
  }
  validateTime(input.now);
  const name = normalizedName(input.name);
  return validateBooklists({
    version: 1,
    lists: [
      ...document.lists,
      {
        id: input.id,
        name,
        createdAt: input.now,
        updatedAt: input.now,
        archived: false,
        members: [],
      },
    ],
  });
}

export function renameBooklist(
  document: BooklistsDocument,
  id: string,
  name: string,
  now: number,
): BooklistsDocument {
  const normalized = normalizedName(name);
  return changeList(document, id, now, (list, timestamp) =>
    list.name === normalized
      ? list
      : { ...list, name: normalized, updatedAt: timestamp },
  );
}

export function addBooklistMembers(
  document: BooklistsDocument,
  id: string,
  members: readonly WorkReference[],
  now: number,
): BooklistsDocument {
  const incoming = checkedReferences(members);
  return changeList(document, id, now, (list, timestamp) => {
    if (list.archived) throw new Error("请先恢复书单，再整理作品。");
    const existing = new Set(list.members.map(referenceKey));
    const additions = incoming.filter(
      (member) => !existing.has(referenceKey(member)),
    );
    if (!additions.length) return list;
    return {
      ...list,
      members: [...list.members, ...additions],
      updatedAt: timestamp,
    };
  });
}

export function removeBooklistMembers(
  document: BooklistsDocument,
  id: string,
  members: readonly WorkReference[],
  now: number,
): BooklistsDocument {
  const keys = new Set(checkedReferences(members).map(referenceKey));
  return changeList(document, id, now, (list, timestamp) => {
    if (list.archived) throw new Error("请先恢复书单，再整理作品。");
    const remaining = list.members.filter(
      (member) => !keys.has(referenceKey(member)),
    );
    if (remaining.length === list.members.length) return list;
    return { ...list, members: remaining, updatedAt: timestamp };
  });
}

export function archiveBooklist(
  document: BooklistsDocument,
  id: string,
  now: number,
): BooklistsDocument {
  return changeList(document, id, now, (list, timestamp) =>
    list.archived ? list : { ...list, archived: true, updatedAt: timestamp },
  );
}

export function restoreBooklist(
  document: BooklistsDocument,
  id: string,
  now: number,
): BooklistsDocument {
  return changeList(document, id, now, (list, timestamp) => {
    if (!list.archived) return list;
    if (
      document.lists.some(
        (other) => !other.archived && other.name === list.name,
      )
    ) {
      throw new Error("已有同名书单，请先改名再恢复。");
    }
    return { ...list, archived: false, updatedAt: timestamp };
  });
}
