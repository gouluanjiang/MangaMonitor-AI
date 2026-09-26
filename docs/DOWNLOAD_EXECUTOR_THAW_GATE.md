# Download behavior and authority review

Use `DEVELOPMENT_HANDOFF.md` and its linked gates for current authority. The initial 2026-09-08 staging review and later desktop reviews cover different scopes; this document grants no execution permission.

## Trigger

This gate applies before any change that can make the system do one or more of the following:

- perform a real JM/Pica manga/image download for execution rather than read-only validation;
- materialize downloaded media into a user library;
- promote command staging into the managed library;
- replace an existing version;
- mark a pending task completed based on download execution;
- physically delete files.

## Review scope and reusable evidence

Read the affected contract, implementation/tests and `upstream-source-reference.md` sections. Use `V1_ADD_ONLY_DOWNLOAD_THAW_2026-09-08.md` for the initial staging review or the handoff's applicable desktop review.

Reuse evidence while its pins, contracts, behavior and authorized scope apply. Reopen affected upstream source for changed pins/protocols, newly adopted behavior or defects that undermine the review. A new session or unrelated documentation/UI edit does not trigger all three upstream reviews.

Authority expansion requires an explicit decision and review of the affected gates in that session. Code review and upstream completion semantics grant no live execution or state-transition authority.

## Upstream topics to check when affected

### JM

For affected JM behavior, check:

- API-domain and retry behavior;
- `/chapter`, `/chapter_view_template`, scramble ID, and block-number/image unscrambling rules;
- exact chapter/image enumeration behavior;
- image format handling and empty/corrupt response behavior;
- concurrency, pause/resume, retry, and partial-download behavior;
- upstream definition of chapter/download completion.

### Pica

For affected Pica behavior, check:

- authentication/token lifetime and refresh behavior;
- signed API request constants/headers;
- chapter and image pagination;
- file-server/path construction;
- retry/backoff and concurrency behavior;
- temporary-directory and finalization behavior;
- upstream definition of chapter/download completion.

### Cross-source reliability

Re-check the relevant patterns from `JMComic-Crawler-Python`, including bounded retry, domain failover, domain health/cooldown, and caching. Any adopted retry must preserve physical request accounting and checkpoint semantics.

## MangaMonitor-AI rules that upstream code must not weaken

Even when upstream implementation code is reused, the following remain independent MangaMonitor-AI requirements:

- exact current user approval generation remains authoritative;
- task revision, target hash, source namespace/site ID, and local file identity remain bound end to end;
- command output stays isolated under `commands/<command_id>` until a separate promotion gate permits otherwise;
- pre-existing destinations are not overwritten by staging execution;
- partial staging is not automatically deleted;
- source enumeration/pagination must be proven complete before completion can be claimed;
- all scheduled download work must be joined/completed before success;
- staged files must be verified against the exact manifest/proof chain;
- staging success does not imply inventory mutation;
- inventory verification does not imply task completion;
- task completion does not imply promotion/replacement/deletion;
- replacement and physical deletion require separate explicit authority;
- source/network/auth failures do not prove a work unavailable;
- `production_enabled=false` remains closed until explicit acceptance is complete.

## Verification and acceptance

Implement within the reviewed scope, updating meaningful regressions for changed behavior. Run affected checks and required CI under `AGENTS.md`; do not repeat unchanged validation stages.

Live checks require applicable authorization and local execution. Preserve each gate's staging/proof and human-acceptance prerequisites; synthetic tests do not satisfy them. Inventory, promotion, replacement and deletion remain separately gated; production requires explicit final acceptance.

Missing approval or integrity evidence blocks the affected real action. Identify the gap and continue independent authorized work.
