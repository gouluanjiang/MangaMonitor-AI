# Mandatory download-executor thaw gate

The real local manga download executor is currently frozen. This document exists so future work does not depend on conversational memory when the project eventually resumes real download execution.

## Trigger

This gate applies before any change that can make the system do one or more of the following:

- perform a real JM/Pica manga/image download for execution rather than read-only validation;
- materialize downloaded media into a user library;
- promote command staging into the managed library;
- replace an existing version;
- mark a pending task completed based on download execution;
- physically delete files.

## Required reading before thawing

The developer or assistant must first read and re-validate:

1. `AGENTS.md`.
2. `docs/upstream-source-reference.md`.
3. The current A6 executor/staging/completion result documents and the actual current source implementing those contracts.
4. The three upstream repositories at the pinned revisions recorded in `docs/upstream-source-reference.md`, with special attention to their download managers, source enumeration, pagination, image transforms, retries, concurrency, temporary-directory behavior, and completion conditions.

The three upstream projects are implementation references, not authority for MangaMonitor-AI state transitions.

## Upstream knowledge that must be reconsidered at thaw time

### JM

Re-check at least:

- API-domain and retry behavior;
- `/chapter`, `/chapter_view_template`, scramble ID, and block-number/image unscrambling rules;
- exact chapter/image enumeration behavior;
- image format handling and empty/corrupt response behavior;
- concurrency, pause/resume, retry, and partial-download behavior;
- upstream definition of chapter/download completion.

### Pica

Re-check at least:

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

## Required engineering sequence after a future thaw decision

1. Record the explicit decision that real download execution is no longer frozen.
2. Refresh the upstream review and update pinned revisions/documentation if necessary.
3. Add/refresh protocol and completion regression tests before changing execution code.
4. Implement the smallest source-specific bridge compatible with current safety contracts.
5. Validate offline/unit tests first.
6. Run guarded live source tests without library mutation.
7. Validate staging-only execution and proof-chain behavior.
8. Perform human acceptance of real downloads into isolated staging.
9. Only then consider separate inventory/promotion/replacement gates.
10. Keep production disabled until the project's final production acceptance criteria pass.

If any prerequisite is ambiguous or stale, fail closed and re-audit instead of assuming old upstream behavior is still valid.
