# Assistant A6.5 — source completion proof results

Status: **PASS candidate** on `assistant-a6-source-completion`.

Baseline: `main@cf4dac986ea197b7ae4af07b68db11b9a4b1849a` (accepted A6.4).

## Goal

A6.5 defines the offline source-specific proof that a future JM/Pica downloader bridge must produce before it can create an accepted A6.3 staging manifest. It does not execute either downloader.

## Pinned source contracts

- JM upstream: `f0cdd724af6892002f2fb7be883b88832cebe7e9`
- Pica upstream: `77c8b62ede42b3afc074506d092313816af8092d`

Evidence claiming another upstream commit fails closed.

## JM completion barrier

A successful GUI/Tauri task-creation return is not accepted as completion. Every expected chapter must:

- belong to a fully enumerated full-work scope;
- be scheduled;
- be joined;
- reach terminal `COMPLETED`;
- report nonzero exact expected/completed image counts;
- report zero failed images;
- bind exactly one staged content artifact per expected image.

This models the stronger internal completion point observed in the pinned JM worker rather than the weaker task-spawn return.

## Pica completion barrier

In addition to the chapter terminal/image rules above, Pica requires:

- a complete whole-work chapter pagination proof;
- a complete image pagination proof for every chapter;
- `successful_pages` exactly equal to `1..=total_pages`;
- zero failed pages.

This explicitly blocks the pinned upstream `utils.rs::get_comic` behavior where a later failed chapter page is logged and omitted while the helper can still return a partial comic.

## Cross-source invariants

- exact A6.2 command/task/work/revision/target/source-work binding;
- v1 scope fixed to `FULL_SOURCE_WORK`;
- deterministic canonical chapter ordering with no duplicate IDs/orders;
- exact expected chapter count;
- all scheduled chapter work joined;
- exact content artifact ownership with no duplicates/unassigned files;
- per-chapter content artifact count equals expected image count;
- A6.3 manifest validation is reused before a proof is returned;
- output `execution_supported=false`;
- inventory/task completion/promotion/replacement/physical delete authorities remain false.

## Validation

Final branch CI: `34082559259` — **SUCCESS**.

Passed:

- complete workspace regression suite;
- JM complete transcript positive case;
- JM create/spawn-only negative case;
- JM incomplete image/chapter negative cases;
- Pica complete pagination positive case;
- Pica later chapter-page failure negative case;
- Pica later image-page failure negative case;
- missing/duplicate/unassigned chapter/artifact negative cases;
- exact image-to-artifact count binding;
- upstream commit mismatch and forged backend/binding negatives;
- offline source-completion CLI regression;
- Clippy with warnings denied;
- assistant offline/read-only guard;
- `production_enabled=false` guard.

## Safety boundary

A6.5 makes no JM/Pica request, uses no Pica credential, downloads no image, writes no staging manga file, mutates no monitor state, completes no pending task, promotes/replaces nothing, and deletes nothing.

The next safe subphase is A6.6: add a disabled source-bridge execution request/spec layer that translates an exact local plan into pinned JM/Pica bridge requirements without enabling network execution. Real source execution remains a later explicit gate.
