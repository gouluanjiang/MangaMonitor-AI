# A6.14A Results — Guarded Live Media Byte Fetch

## Status

A6.14A is implementation-complete on `assistant-a6-live-media-fetch` and has passed final pull-request baseline validation.

Validated final executable head before this results-only update:

- `d3bdfeeb8fa809827aada0e8d16a6905bd1756a8`
- baseline CI run: `34097896255`
- Workspace tests: success
- Build assistant view CLI: success
- Clippy with warnings denied: success
- Assistant runtime remains read-only and offline: success
- Production gate remains closed: success

This results update is outside the baseline CI path filter and does not change executable behavior.

## Scope implemented

A6.14A connects the exact A6.13 source-media descriptor set to the already-audited A6.12 command-owned staging executor with real media HTTP GETs for the subset whose transformation behavior is currently provable.

The production execution path is:

`A6.10 current authorization generation -> A6.11/A6.13 exact descriptor validation -> cloud-monitor private live media transport -> source-format validation -> A6.12 reauthorization/write/completion chain`

A6.14A adds no new inventory, task-completion, promotion, replacement, or deletion authority.

## Private transport boundary

Raw production media transport is owned by the private `cloud-monitor::live_media_transport` module and is reached through the guarded `live_media_fetch::execute_live` path.

The earlier adapter-local transport helpers are now compiled only under `#[cfg(test)]`. They remain available for adapter protocol regression coverage but are absent from normal adapter builds, so they are not an external raw-fetch API and cannot bypass the guarded A6.14 cloud path.

### JM

The guarded JM transport:

- accepts only exact HTTPS URLs on the pinned A6.13 JM image domain;
- requires the exact `/media/photos/` path family;
- rejects query strings, fragments, embedded credentials, explicit ports, alternate hosts, and URL normalization drift;
- uses the pinned upstream user agent but carries no JM API/session credential;
- disables redirects and performs no hidden retry;
- enforces connect/request timeouts;
- bounds the response body to 128 MiB and rejects empty responses;
- requires HTTP 200 before bytes can enter staging processing.

A6.14A supports only JM transformations that are exact byte no-ops:

- GIF with `NONE / 0`;
- WEBP with `JM_SCRAMBLE_BLOCKS / 0`.

A positive JM scramble parameter fails closed before network access or command-tree creation with `JM_LIVE_MEDIA_PIXEL_TRANSFORM_NOT_ENABLED`. A6.14A does not pretend compressed WEBP bytes have been correctly pixel-transformed without a separately audited decoder/encoder implementation.

### Pica

The guarded Pica transport:

- accepts only exact HTTPS provider storage hosts in the audited `storage*.picacomic.com` family;
- requires the exact `/static/` path family carried by the validated descriptor;
- rejects query strings, fragments, embedded credentials, explicit ports, API host URLs, localhost/IP hosts, alternate domains, and URL normalization drift;
- has no Pica token parameter and does not attach API credentials, cookies, signature headers, nonce, or authorization to media requests;
- disables redirects and performs no hidden retry;
- enforces connect/request timeouts;
- bounds the response body to 128 MiB and rejects empty responses;
- requires HTTP 200 before bytes can enter staging processing.

Pica A6.14A media is transform-free only (`NONE / 0`).

## Response/content validation

Before A6.12 receives a `ProcessedMedia` object, A6.14A checks source-format magic for the descriptor-bound format:

- GIF87a/GIF89a;
- RIFF/WEBP;
- JPEG SOI;
- PNG signature.

HTML/error payloads or mismatched content therefore fail before staging writes.

## A6.12 safety invariants preserved

The live fetch bridge delegates filesystem mutation and completion proof to A6.12. Existing invariants remain unchanged:

- reauthorization immediately before each fetch/write stage;
- exact command/task/work/revision/target binding;
- command-owned `commands/<command_id>` staging only;
- create-new/no-overwrite semantics;
- partial output is not silently promoted or treated as completion;
- manifest + real-filesystem completion verification remains mandatory;
- inventory mutation authorization: false;
- task completion authorization: false;
- promotion authorization: false;
- replacement authorization: false;
- physical delete authorization: false.

## Dependency hygiene

`cloud-monitor` now owns its direct `reqwest` dependency because the guarded production media transport lives there.

The lock update was deliberately kept minimal. Cargo initially proposed an unrelated `ipnet 2.12.1 -> 2.12.2` refresh while regenerating the lock; that drift was reverted with the pinned Rust/Cargo toolchain. The final `Cargo.lock` delta for this dependency edge is only the new `cloud-monitor -> reqwest` direct dependency entry; no unrelated transitive upgrade remains.

## Regression coverage

A6.14A adds/retains coverage for:

- exact JM request URL and pinned user agent;
- no JM API/session credential headers;
- exact Pica request URL with no API token/credential headers;
- Pica metadata-controlled SSRF/credential URL rejection;
- query/fragment/credential/port/alternate-host rejection;
- bounded response-size arithmetic;
- transform-free Pica processing;
- JM GIF and zero-block WEBP exact no-op processing;
- positive JM scramble failing before transport, reauthorization, or staging-tree creation;
- source magic mismatch failing before A6.12 writes;
- legacy adapter raw transports compile only for regression-test targets;
- all prior workspace regressions and production-closed guards.

## Safety boundary after A6.14A

A6.14A is not production enablement.

Still closed or incomplete:

- positive JM WEBP scramble transformation;
- a fully validated end-to-end Windows production executor flow;
- inventory mutation/task completion after a successful staging receipt;
- promotion/replacement/deletion authority;
- production configuration enablement.

`production_enabled=false` remains mandatory.

## Recommended next safe sub-stage

The next implementation stage should keep the same authorization and staging boundary while addressing the remaining real-execution gap, beginning with the positive JM WEBP transform only if it can be reproduced and regression-tested against the pinned upstream behavior with an audited image decode/encode path.

No later stage should open production merely because media bytes can now be fetched.
