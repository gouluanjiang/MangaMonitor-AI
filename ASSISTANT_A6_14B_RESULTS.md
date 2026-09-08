# A6.14B Results — Positive JM WEBP Pixel Transform

## Status

A6.14B is implementation-complete on `assistant-a6-jm-positive-transform` and has passed the normal baseline validation on the final executable candidate.

Validated executable head before this results-only commit:

- `01770c5277be11f2e3eb3fc82f3af412de746730`
- baseline CI run: `34099422482`
- Workspace tests (`--locked`): success
- Build assistant view CLI: success
- Clippy with warnings denied: success
- Assistant runtime remains read-only and offline: success
- Production gate remains closed: success

This results document is outside the baseline CI executable path filter and does not change runtime behavior.

## Scope implemented

A6.14B closes the positive JM WEBP transform gap left intentionally fail-closed in A6.14A.

The guarded execution chain remains:

`A6.10 current authorization generation -> A6.11/A6.13 exact descriptor validation -> private A6.14 media transport -> source-format validation -> descriptor-bound JM pixel transform -> A6.12 command-owned staging/completion chain`

No inventory, task-completion, promotion, replacement, deletion, or production authority is added.

## Pinned upstream behavior

The implementation is based on the pinned JM upstream:

`lanyeeee/jmcomic-downloader@f0cdd724af6892002f2fb7be883b88832cebe7e9`

Relevant upstream behavior is in:

`src-tauri/src/downloader/download_img_task.rs`

The pinned algorithm:

1. decodes the downloaded non-GIF image to RGB pixels;
2. receives the already-calculated `block_num` bound by A6.13;
3. reconstructs the image by moving source blocks from the bottom toward the top into output order;
4. preserves the upstream remainder-row rule: `height % block_num` rows belong to the first output block;
5. re-encodes the reconstructed pixels.

A6.14B reproduces that block geometry rather than deriving or guessing a transform from downloaded bytes.

## Private transform boundary

The transform implementation lives in private module:

`cloud-monitor::jm_media_transform`

It is not a public raw-image processing API.

Only the guarded `live_media_fetch` path invokes it after:

- exact source-media descriptor validation;
- source-format magic validation;
- source selection and exact media transport;
- descriptor-bound transform name/parameter validation.

Positive transforms are accepted only for:

`JM + WEBP + JM_SCRAMBLE_BLOCKS + descriptor-bound block count`

JM GIF remains `NONE / 0`. Pica remains transform-free `NONE / 0`.

## Image dependency scope

The project pins:

`image = 0.25.5`

with default features disabled and only the `webp` feature enabled for A6.14B.

This is narrower than the pinned upstream's JPEG/PNG/WEBP image feature set because A6.14B needs only the JM WEBP decode/re-encode path.

Cargo lock resolution was performed with the pinned Rust `1.98.1` toolchain using the existing lock as the starting point. The final lock delta adds the required image/WebP dependency chain without deleting or refreshing existing locked packages.

The temporary write-permission lock-refresh workflow was removed before final candidate validation and is not part of the proposed final tree.

## Fail-closed behavior

A6.14B rejects or fails safely on:

- unsupported source/format/transform combinations;
- transform parameters that cannot fit the expected block-count type;
- malformed or non-decodable WEBP payloads even if superficial RIFF/WEBP magic is present;
- invalid image/block geometry;
- WEBP encoding failure;
- invalid encoded WEBP output;
- content-magic mismatch before transformation or staging write.

The A6.14A exact media URL, credential-isolation, redirect/retry, timeout, response-size, and private transport restrictions remain unchanged.

## Regression coverage

A6.14B adds or updates regression coverage for:

- pinned two-block bottom-first row reconstruction;
- pinned remainder-row placement;
- zero-block WEBP exact byte no-op;
- unsupported transform fail-closed behavior;
- positive JM scramble accepted only for descriptor-bound WEBP transforms;
- superficially WEBP-like but undecodable payload rejection;
- source magic mismatch before transform/write;
- all existing workspace regressions;
- read-only/offline assistant runtime guard;
- production gate remaining closed.

The previous A6.14A test fixture whose purpose was to prove that every positive JM scramble must be rejected was removed because A6.14B intentionally changes that exact capability. A6.10 authorization and A6.12 staging/binding regressions remain covered in their owning modules.

## Safety boundary after A6.14B

A6.14B is still not production enablement.

The following remain separate later gates:

- fully validated end-to-end local/Windows execution orchestration;
- inventory mutation after a verified staging receipt;
- task-completion transition;
- promotion into the managed library;
- replacement of an older version;
- physical deletion;
- production configuration enablement.

`production_enabled=false` remains mandatory.

## Mandatory pre-production author-search acceptance gate

After all remaining implementation stages are complete, the system must **not** be formally enabled immediately.

Before production enablement, perform real author-search acceptance testing with at least:

1. **single-author retrieval test** — one author query through the intended live search path;
2. **multi-author retrieval test** — multiple authors in one acceptance session/batch, including result separation and deduplication behavior.

The acceptance must verify, as applicable to the final execution path:

- each requested author is searched against the intended live source(s);
- returned works map to the correct author/query rather than leaking across authors;
- duplicate works/source identities are handled deterministically;
- ambiguous or unknown matches fail closed instead of silently selecting a work;
- pagination/enumeration completes according to the existing source contracts;
- matcher/rule outputs remain deterministic;
- resulting monitor/task state changes are exactly the authorized changes and no unrelated state is mutated;
- any download test remains staging-first and respects all approval/completion gates;
- failed/partial searches or downloads do not become successful inventory/task completion;
- production remains disabled throughout the acceptance test itself.

Only after these author-search tests and the remaining implementation/security gates pass should formal production enablement be considered.

## Recommended next stage

Continue the remaining local execution and state-transition gates while keeping production closed. The final rollout stage must end with the author-search acceptance gate above rather than enabling production solely because implementation and CI are green.
