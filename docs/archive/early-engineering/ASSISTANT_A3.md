# Assistant A3 — controlled author registry management

## Goal

A3 prepares the first bounded write capability for the optional ChatGPT layer: managing which authors are actively monitored.

The intended user interaction is natural language, for example:

- “把 XXX 作者加进监控。”
- “不要再监控 XXX。”
- “把 XXX 恢复监控。”

ChatGPT may translate that intent into an author-registry operation, but deterministic Rust validates and stages the resulting registry change.

## Historical preservation rule

“Remove author” means **disable future monitoring**, not erase history.

Disabling an author must not delete:

- catalog records;
- inventory/work IDs;
- decisions;
- review history;
- pending history;
- source mappings.

Re-enabling the author preserves the same registry entry and author ID.

## Operations

A3 supports exactly:

- `add <name>` — add a new monitored author, or re-enable an existing canonical match;
- `enable <name>` — enable exactly one existing canonical match;
- `disable <name>` — disable exactly one existing canonical match.

No bulk delete and no physical history removal are supported.

## Canonical duplicate check

Registry duplicate detection reuses the same conservative author-key semantics already used by production author evidence:

- trim;
- existing conservative Unicode/title normalization;
- the already-frozen closed `10駅` / `10驛` equivalence;
- no fuzzy matching;
- no general Simplified/Traditional conversion;
- no new alias expansion.

If two registry entries collapse to the same canonical key, the operation fails closed rather than choosing one.

## IDs for newly user-added authors

Existing imported author IDs are preserved exactly.

A newly added author receives a stable ID derived from the canonical author key:

`AUTHOR_USER_<SHA256-prefix>`

The deterministic ID avoids list-position renumbering and remains stable if unrelated authors are added/disabled later. A hash collision with another existing ID fails closed.

## Staging semantics

`assistant-author-edit` reads one `authors.json` and writes into a **new output directory only**:

- `authors.json` — complete proposed registry document;
- `author-change.json` — audit describing requested operation and outcome.

The input file is never changed. Existing output paths are refused.

A later publication layer may commit the staged `authors.json` using commit-aware GitHub state logic. A3 itself never commits, pushes, scans sources, or edits any other state file.

## No-op semantics

- adding an already-enabled canonical author is a deterministic no-op;
- adding a disabled canonical author re-enables the existing entry and preserves its original ID/name;
- enabling an already-enabled author is a no-op;
- disabling an already-disabled author is a no-op;
- enabling/disabling an unknown author fails closed.

No-op results are auditable and do not invent a new author entry.

## Analysis interaction

The existing MangaMonitor analysis context already includes `authors.json`. Once a valid registry change is eventually published, the next deterministic state cycle can reanalyse existing catalog records against the changed official-author registry without requiring source requests first.

A3 does not itself run that reanalysis.

## Acceptance criteria

A3 is accepted when tests prove:

- imported author IDs/names are preserved;
- add creates exactly one stable new entry;
- repeated add is idempotent;
- disable preserves the entry and only changes `enabled`;
- re-add/enable preserves the original ID;
- unknown enable/disable fails closed;
- canonical duplicate input fails closed;
- `10駅` / `10驛` duplicate ambiguity fails closed;
- empty names fail closed;
- existing output is refused;
- input bytes remain identical;
- no network access is required;
- all prior tests and clippy continue to pass;
- `production_enabled=false` remains unchanged.

A3 does **not** populate the formal full author registry yet and does not perform a real source scan.
