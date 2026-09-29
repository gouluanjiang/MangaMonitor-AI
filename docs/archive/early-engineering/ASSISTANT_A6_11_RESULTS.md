# Assistant A6.11 — exact source media descriptor results

Status: **PASS candidate** on `assistant-a6-media-descriptors`.

Baseline: `main@b3dae6360ad0bd43f85555582d2e314b5b8600c2` (accepted A6.10).

## Goal

A6.11 narrows A6.10 image-download authority into an exact deterministic media work set before any image-byte transfer or staging write occurs.

## Accepted binding chain

Descriptor validation now requires the full accepted chain:

1. current non-transferable A6.10 `ImageDownloadAuthorization`;
2. original A6.7 `SourcePreflightEvidence`;
3. exact A6.7 `SourcePreflightProof` derived from that evidence;
4. A6.11 `SourceMediaDescriptorSet`.

The validator recomputes the original preflight evidence hash and requires it to equal the A6.10 `preflight_hash`. It then checks the proof reproduces the exact evidence chapter pagination, chapter IDs/order, per-chapter image counts, and aggregate content-unit count. This prevents a caller from forging both proof and descriptors while merely copying an old hash string.

## Exact media and staging scope

Each media descriptor is bound to:

- exact command/task/work/revision/target/source/source-work generation;
- exact preflight hash;
- exact chapter identity and order;
- exact image index and source media identity;
- exact request URL and source format;
- exact required source transform;
- deterministic command-owned staging path.

The canonical relative path is:

`chapters/<chapter_order:06>-<chapter_id>/<image_index:06>.<source_format>`

Absolute paths, parent traversal, backslashes, unsafe Windows path segments, skipped/reordered indexes, duplicate media IDs, duplicate/case-colliding paths, and chapter-scope drift fail closed.

## JM pinned rules

Pinned JM upstream remains `f0cdd724af6892002f2fb7be883b88832cebe7e9`.

Accepted JM image URLs are restricted to the pinned CDN path shape under the expected chapter ID. GIF uses no transform. WEBP requires `JM_SCRAMBLE_BLOCKS` with the exact block count recomputed from the pinned worker algorithm.

The block-count calculation is implemented locally without adding a new dependency and is covered by standard MD5 vectors plus threshold regression tests. Alternate hosts, credentials/query/fragment injection, filename/format mismatch, unsupported formats, and incorrect scramble parameters fail closed.

## Pica pinned rules

Pinned Pica upstream remains `77c8b62ede42b3afc074506d092313816af8092d`.

Pica descriptors require canonical 24-hex media IDs, HTTPS `/static/` media URLs without embedded credentials/query/fragment, supported image extensions matching the URL, and no transform.

## Authority boundary

A6.11 performs metadata validation only. It still does not:

- download image bytes;
- create staging directories or write files;
- mutate inventory;
- complete tasks;
- promote or replace content;
- physically delete anything.

All downstream mutation/task-completion/promotion/replacement/delete authority remains false, and the repository production gate remains closed.

## Validation

Final code CI: `34088203859` — **SUCCESS** on code commit `5b6782247ca4d3430c8d715d033ddfec39d35ad1`.

Passed:

- complete workspace regression suite;
- exact A6.10/A6.7 evidence/proof/descriptor binding;
- forged proof + descriptor reuse of an old preflight hash fails closed;
- exact chapter identity/order/per-chapter image-count checks;
- deterministic staging path and duplicate/collision checks;
- JM exact pinned URL/format/scramble validation;
- standard MD5 and JM scramble-threshold tests;
- Pica credential/query/format/downstream-authority rejection;
- assistant runtime remains offline/read-only;
- Clippy with warnings denied;
- `production_enabled=false` guard.

## Next safe phase

A6.12 may introduce the first isolated media-byte transfer into the exact command-owned staging paths, but only by consuming the current A6.10 generation plus the fully validated A6.11 descriptor set. It must remain unable to mutate inventory, complete tasks, promote/replace content, or delete anything, and it must emit completion/artifact evidence compatible with the existing A6.5/A6.3/A6.4 verification chain.
