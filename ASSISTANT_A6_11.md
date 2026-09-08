# Assistant A6.11 — exact source media descriptors and staging paths

Status: implementation/validation on `assistant-a6-media-descriptors`.

Baseline: `main@b3dae6360ad0bd43f85555582d2e314b5b8600c2` (accepted A6.10).

## Goal

A6.11 turns A6.10's broad permission to download the accepted source scope into an exact, deterministic media work set. It still performs no image-byte download and no filesystem write.

Every later image transfer must be represented by one validated descriptor bound to:

- exact A6.10 authorization generation;
- exact A6.7 source-preflight evidence, proof, and `preflight_hash`;
- exact chapter identity/order and per-chapter expected image count;
- exact source media identity and request URL;
- exact source format and required source transform;
- exact deterministic path relative to command-owned staging.

A6.11 recomputes the hash of the original A6.7 evidence and requires it to equal the non-transferable A6.10 authorization's `preflight_hash`. The A6.7 proof must then exactly reproduce that evidence's chapter pagination, chapter list, per-chapter expected image counts, and total content units. Matching only a copied hash string, chapter total, or aggregate image total is insufficient.

## Deterministic staging paths

Every media item is assigned exactly:

`chapters/<chapter_order:06>-<chapter_id>/<image_index:06>.<source_format>`

Paths are portable relative paths below the existing A6.2 command root `commands/<command_id>`. Absolute paths, parent traversal, backslashes, unsafe segments, duplicate/case-colliding paths, skipped/reordered image indexes, duplicate media IDs, and non-canonical chapter ordering fail closed.

## JM pinned descriptor shape

Pinned JM upstream remains `f0cdd724af6892002f2fb7be883b88832cebe7e9`.

The pinned worker constructs chapter image URLs as:

`https://cdn-msp2.jmapiproxy2.cc/media/photos/<chapter_id>/<filename>`

Only GIF and WEBP image entries belong to the accepted worker schedule. Each JM chapter descriptor carries the exact scramble ID observed from `/chapter_view_template`. GIF requires no scramble transform. WEBP requires `JM_SCRAMBLE_BLOCKS`, and A6.11 recomputes the transform parameter using the pinned upstream algorithm from the scramble ID, chapter ID, and filename stem. A merely plausible block count is not accepted.

A6.11 rejects alternate JM hosts, credentials/query/fragment injection, filename/path mismatch, URL/format mismatch, unsupported formats, missing scramble IDs, or any transform parameter that differs from the pinned calculation.

## Pica pinned descriptor shape

Pinned Pica upstream remains `77c8b62ede42b3afc074506d092313816af8092d`.

The pinned worker derives each image request URL from the source image metadata as:

`<file_server>/static/<media.path>`

A6.11 requires HTTPS media URLs without embedded credentials, queries, or fragments; a `/static/` path; canonical 24-hex image IDs; a supported image extension matching the URL; no JM scramble ID; and no transform.

## A6.10 + A6.7 binding

Descriptor validation requires a current typed `ImageDownloadAuthorization` with:

- live preflight generation verified;
- image download authorized;
- command-owned staging write authorized;
- `reusable_permit=false`;
- all downstream inventory/task/promotion/replacement/delete capabilities still false.

The exact A6.7 `SourcePreflightEvidence` must hash to the A6.10 `preflight_hash`, remain enumeration-only, and match the authorization's exact task/source generation. The exact A6.7 `SourcePreflightProof` must then match both that evidence and the authorization on command/task/work/revision/target/source/source-work, pagination, chapters, `preflight_hash`, expected chapter count, and expected content-unit count, while retaining all download/staging/downstream authority flags false.

The descriptor set must finally repeat the exact A6.10 command/task/work/revision/target/source/source-work, `preflight_hash`, expected chapter/content counts, staging subdir, and write scope, and its chapters must match the verified A6.7 scope exactly.

This closes the forged-proof case where a caller might otherwise alter both proof chapters and descriptors while merely copying an old `preflight_hash` string.

## Authority boundary

A6.11 validates metadata only. It does not:

- perform any image HTTP request;
- create a directory or write a file;
- mutate inventory;
- complete a task;
- promote or replace content;
- delete anything.

The next execution phase may consume only a validated A6.11 descriptor set, its exact original A6.7 evidence/proof, and the still-current A6.10 generation to transfer media bytes into the exact command staging paths and emit observed completion/artifact evidence.
