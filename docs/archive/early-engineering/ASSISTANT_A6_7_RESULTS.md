# Assistant A6.7 — source enumeration preflight results

Status: **PASS candidate** on `assistant-a6-source-preflight`.

Baseline: `main@182193c4c1bdd09b4f9d3576267763b4cf38570e` (accepted A6.6).

## Goal

A6.7 establishes an exact expected source scope before any image byte may be downloaded. It binds complete pinned-source chapter/image enumeration to an exact A6.6 request, while keeping image download, staging writes, inventory/task mutation, promotion, replacement, and deletion closed.

## Expected-scope proof

`SourcePreflightEvidence` is bound to the exact A6.2/A6.6 command generation:

- command/task/work/revision/target hash;
- source and source work ID;
- pinned upstream commit;
- A6.5 completion-contract version;
- `FULL_SOURCE_WORK` scope.

The accepted proof preserves the complete canonical expected scope rather than aggregate counts only:

- deterministic `preflight_hash` over the full evidence;
- exact chapter pagination when required;
- exact ordered chapter IDs/orders;
- exact per-chapter expected image counts;
- exact per-chapter image pagination when required;
- exact total expected content units.

This prevents a later download with the same aggregate image count but different chapter content from passing.

## Preflight-to-completion binding

A6.7 adds an offline comparator between the expected preflight scope and a later A6.5 source-completion transcript. The transcript must preserve the exact:

- task/source/upstream/scope generation;
- chapter count and whole-work pagination;
- chapter IDs and orders;
- per-chapter expected image counts;
- per-chapter Pica image pagination.

Equal aggregate totals are explicitly insufficient. Tests prove that redistributing the same total image count between chapters or changing a chapter ID fails closed.

This scope comparator does not replace the independent A6.5 completion normalizer; a real downloader must still prove scheduled/joined/terminal completion, artifact ownership, hashes, and all other A6.5/A6.3/A6.4 barriers.

## JM pinned enumeration primitive

`jm-adapter` remains pinned to `f0cdd724af6892002f2fb7be883b88832cebe7e9` and adds read-only preflight primitives:

- `/album` chapter enumeration using the pinned upstream series-to-chapter rule;
- the pinned single-chapter fallback when `series` is empty;
- malformed/duplicate chapter IDs fail closed rather than being silently dropped;
- `/chapter` image enumeration/counting using the pinned worker's GIF/WEBP scheduling rule;
- malformed non-string image entries fail closed;
- no image media URL is requested and no image byte is downloaded.

## Pica pinned enumeration primitive

`pica-adapter` remains pinned to `77c8b62ede42b3afc074506d092313816af8092d` and adds read-only preflight primitives:

- `comics/<id>/eps?page=N` whole-work chapter pagination;
- `comics/<id>/order/<order>/pages?page=N` per-chapter image pagination;
- every page through the exact reported final page must succeed;
- reported page counts may not change mid-enumeration;
- empty intermediate/final enumeration pages fail closed;
- chapter identities/orders must remain canonical and duplicate-free;
- image identities are duplicate-checked and pinned media metadata shape is required;
- a bounded page budget must reach the reported final page or fail closed.

The Pica token stays private inside the runtime client. No credential value is included in A6.7 proof/request structures, CLI output, fixtures, or repository history.

## Runtime boundary

The adapters now contain explicit source-metadata enumeration methods, but A6.7 intentionally does **not** add a network-capable preflight runner/CLI. Deserializing an old A6.6 request therefore cannot trigger source access.

`assistant-source-preflight-check` is offline only: it validates serialized plan/request/evidence inputs and emits the disabled expected-scope proof without mutating any input bytes.

The next phase must revalidate the current task generation and current user approval immediately before any live source-preflight network read is allowed.

## Validation

Final code CI: `34084621790` — **SUCCESS** on commit `e42ff7832c90156e1713cf6b81cc33fd7cd4ce88`.

Passed:

- complete workspace regression suite;
- JM expected-scope positive and adversarial cases;
- JM series fallback/duplicate/malformed image-entry tests;
- Pica complete chapter/image pagination cases;
- Pica missing/failed/non-canonical pagination cases;
- Pica canonical chapter/media-shape parser tests;
- exact preflight hash binding;
- exact preflight-to-A6.5 completion scope binding;
- equal-total/wrong-per-chapter negative case;
- wrong-chapter-identity negative case;
- offline source-preflight CLI input-byte preservation;
- phase-boundary rejection if evidence claims image bytes or staging writes;
- all mutation/authorization outputs remain false;
- Clippy with warnings denied;
- assistant offline/read-only guard;
- `production_enabled=false` guard.

## Safety boundary

A6.7 authorizes no image-byte download, staging write, inventory mutation, task completion, promotion, replacement, or deletion. It does not add an automatic network execution entry point. The original No-AI repository remains untouched.

The next safe subphase is A6.8: a current-generation source-preflight authorization gate that revalidates the exact task and current user approval before any live metadata enumeration can occur. Real image download remains a later explicit gate.
