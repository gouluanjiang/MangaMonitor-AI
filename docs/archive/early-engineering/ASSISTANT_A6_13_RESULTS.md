# A6.13 Results — Live Source Media Descriptor / Fetch Preparation Layer

## Status

A6.13 is complete on `assistant-a6-live-media-descriptors`.

Validated code head before this results-only commit:

- `10342e2afcfb69d48d86b8b516a5fd6d6466886c`
- CI run: `34092378730`
- Workspace tests: success
- Build assistant view CLI: success
- Clippy: success
- Assistant runtime remains read-only and offline: success
- Production gate remains closed: success

The results document itself is outside the CI workflow path filters and does not change executable behavior.

## Scope implemented

A6.13 adds a metadata-only live descriptor preparation bridge. It does not fetch media bytes and it does not write staging files.

### JM

Pinned upstream remains:

`lanyeeee/jmcomic-downloader@f0cdd724af6892002f2fb7be883b88832cebe7e9`

The implementation now:

- re-reads `/chapter_view_template` for the exact chapter scramble ID;
- fails closed when the scramble ID is missing, malformed, or zero instead of accepting the upstream fallback;
- re-reads `/chapter` for the exact chapter ID;
- preserves pinned scheduling semantics: only GIF and WEBP media are scheduled;
- binds the exact filename, normalized source format, chapter ID, image index, deterministic staging-relative path, fixed JM CDN URL, and scramble transform parameter;
- preserves GIF as `NONE / 0`;
- reproduces the pinned WEBP block-count algorithm exactly, including the upstream MD5-hex last-character ASCII behavior;
- uses `Path::file_stem()` semantics so uppercase `.WEBP` input is handled consistently with pinned upstream behavior;
- rejects duplicate or unsafe scheduled filenames;
- revalidates the exact A6.10 authorization generation immediately before both source metadata requests for every chapter.

### Pica

Pinned upstream remains:

`lanyeeee/picacomic-downloader@77c8b62ede42b3afc074506d092313816af8092d`

The implementation now:

- re-reads every `comics/<comic_id>/order/<chapter_order>/pages?page=<page>` image metadata page;
- rejects missing, empty, inconsistent, overrun, or budget-exhausted pagination;
- rejects duplicate or malformed image IDs;
- retains exact `_id`, `originalName`, `path`, and `fileServer` source metadata inputs;
- constructs the exact media URL as `<fileServer>/static/<path>`;
- binds image ID, URL, extension, chapter order, image index, deterministic staging-relative path, expected image count, and exact pagination proof;
- keeps the Pica credential only in the live client input; it is not copied into descriptors, proof objects, request traces, or staging metadata;
- revalidates the exact A6.10 authorization generation immediately before every image-metadata pagination request.

## A6.11 / A6.10 binding hardening

During A6.13 review two fail-closed gaps were tightened before merge:

1. JM descriptor validation now follows pinned upstream file-stem semantics rather than requiring a lowercase `.webp` suffix in the original filename.
2. A6.11 now explicitly requires the preflight proof's upstream commit, completion-contract version, and scope to match the exact hashed evidence, in addition to the existing identity, pagination, chapter, and content-count bindings.

The live A6.13 bridge also rejects an authorization-generation change and performs a final exact generation check after descriptor validation.

## Regression coverage

Coverage added or retained for:

- missing/invalid/zero JM scramble IDs;
- pinned JM media filtering/order/block calculation;
- uppercase JM WEBP file-stem behavior;
- unsafe or duplicate JM filenames;
- unsafe Pica file servers, paths, credentials-in-URL forms, and unsupported formats;
- exact JM live descriptor URL/transform construction;
- JM chapter/count drift;
- exact Pica pagination/ID/URL/extension/order construction;
- Pica pagination/count drift;
- A6.10 authorization generation drift;
- existing A6.11 descriptor binding, canonical path, source-specific transform, credential/query, and downstream-authority rejection tests.

## Safety boundary after A6.13

A6.13 does **not** authorize or implement unrestricted production download execution.

The following remain unchanged:

- media bytes are not fetched by A6.13;
- staging writes remain exclusively an A6.12 concern and are limited to a fresh `commands/<command_id>` tree;
- overwrite remains forbidden;
- partial staging output is not automatically deleted;
- staging success does not imply inventory mutation or task completion;
- inventory mutation authorization: false;
- task completion authorization: false;
- promotion authorization: false;
- replacement authorization: false;
- physical delete authorization: false;
- `production_enabled=false`;
- the original No-AI repository is untouched.

Global fail-closed invariants remain in force, including incomplete Pica pagination, `UNKNOWN` handling, size-selection rules, and full chapter/image completion requirements.

## Recommended next safe sub-stage

The next stage should be an isolated **A6.14 live media byte fetch adapter** that plugs the exact A6.13 descriptor set into the already-audited A6.12 command-owned staging executor.

A6.14 should remain narrow:

- fetch only the exact URL in a validated A6.13 descriptor;
- do not attach Pica API credentials to Pica media URL requests;
- re-run A6.10 authorization immediately before each byte fetch (already supported by A6.12 execution flow);
- validate response status/content before write;
- apply only the exact bound JM transform parameter;
- preserve `create_new` / no-overwrite behavior;
- leave partial output in place on failure;
- require A6.5 completion proof + A6.3 manifest + A6.4 real-filesystem verification for staging success;
- keep inventory mutation, task completion, promotion, replacement, and physical deletion closed;
- keep production disabled.
