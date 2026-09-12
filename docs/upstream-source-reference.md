# Upstream source reference for JM/Pica adapters

This note records upstream behaviors that are relevant to MangaMonitor-AI. It is intentionally source-control documentation so the project does not rely on conversational memory for protocol details.

## Sources and pinned revisions

- `hect0x7/JMComic-Crawler-Python` — reviewed at `9fddb0494caf0cdc812ac6cbfc1c62f4f845b058` (v2.7.5 era). MIT License, Copyright (c) 2023 hect0x7.
- `lanyeeee/jmcomic-downloader` — protocol extraction pinned at `f0cdd724af6892002f2fb7be883b88832cebe7e9`. MIT License, Copyright (c) 2024-2026 lanyeeee.
- `lanyeeee/picacomic-downloader` — protocol extraction pinned at `77c8b62ede42b3afc074506d092313816af8092d`. MIT License, Copyright (c) 2024-2026 lanyeeee.

The MIT notices above must remain preserved when substantial upstream code is redistributed.

## JM behaviors worth retaining

### Protocol details already used

- Mobile API token: MD5 of `timestamp + 18comicAPP` with `tokenparam=timestamp,2.0.13` for ordinary API requests.
- `/chapter_view_template` is special: upstream uses the distinct token secret `18comicAPPContent`. This distinction must not be collapsed into the normal API token path.
- API response `data` is Base64 + AES-256-ECB using the MD5-derived `timestamp + 185Hcomic3PAPP7R` key.
- Search can return either a paged search payload or a direct `redirect_aid`; redirect must be treated as an exact detail lookup rather than an empty search page.
- The five lanyeeee API domains currently pinned by this project are all valid upstream choices: `www.cdnzack.cc`, `www.cdnhth.cc`, `www.cdnhth.net`, `www.cdnbea.net`, `www.cdn-mspjmapiproxy.xyz`.
- Scramble/block-number logic and the chapter/image metadata shapes are already pinned in the adapter and remain fail-closed where the upstream GUI uses permissive fallbacks.

### Reliability pattern adopted from JMComic-Crawler-Python

JMComic-Crawler-Python treats domain rotation and retry as first-class client behavior. MangaMonitor-AI adopts only a narrower version:

- retry/fail over only inside the already-pinned domain allowlist;
- primary configured domain stays first;
- failover is allowed for transport failures and domain-level transient HTTP responses only;
- protocol/schema/decryption/API-code failures remain fatal and are never hidden by domain rotation;
- every physical attempt is appended to `RequestTrace`, so the cloud request budget counts failover attempts instead of under-reporting them.

Do **not** dynamically trust domains discovered from remote pages without a separate trust/update process. The crawler supports dynamic domain discovery for end-user convenience, but that is not appropriate for MangaMonitor-AI's pinned-source security model.

### Useful ideas not yet adopted

- Metadata caching can reduce repeated requests, but only if cache keys include source identity and cannot suppress live checks required by the scan/revision safety model.
- More aggressive retry rounds / domain blacklisting exist upstream. Do not adopt them until request-budget semantics and checkpoint timing are explicitly modeled.
- Upstream image fetch retries an empty JM response with a cache-busting query. MangaMonitor-AI's guarded A6.14 exact-media transport intentionally forbids query mutation/retries, so this is a reference signal only, not current behavior.

## Pica behaviors worth retaining

### Protocol details already used

- API host `https://picaapi.picacomic.com/`.
- Request signature is HMAC-SHA256 over lowercased `path + time + nonce + method + api-key`, using the pinned digest key and Android-style request headers.
- Authentication is `POST auth/sign-in`; the returned token is then sent as `authorization` for search/detail/chapter/image metadata requests.
- Advanced search is `POST comics/advanced-search?page=N` with `keyword`, sort, and categories.
- Chapter pagination is `GET comics/{id}/eps?page=N`.
- Image metadata pagination is `GET comics/{id}/order/{chapter_order}/pages?page=N`.
- lanyeeee's client uses bounded transient retry middleware for API and image requests. This is useful for future scaling, but MangaMonitor-AI must preserve physical-attempt accounting and fail-closed unavailable semantics before adopting it wholesale.

### Safety differences deliberately preserved

- The 2026-09-12 desktop Pica acceptance repair adds a bounded same-origin redirect loop after a task-specific header check showed 301 then 200 JPEG. Automatic redirects remain disabled; every initial/redirect GET must receive a fresh authorization from the staging coordinator, at most two redirects are allowed, and original descriptor/checkpoint identity remains unchanged. See `PICA_DOWNLOAD_REDIRECT_FIX_2026-09-12.md`. This is not arbitrary-domain trust or retry/failover authority; JM and the legacy exact-transfer entry remain unchanged.

- Upstream GUI code is optimized for interactive downloading; MangaMonitor-AI requires complete pagination evidence and rejects missing/intermediate pages instead of silently aggregating partial results.
- MangaMonitor-AI does not interpret ordinary HTTP/auth/network failures as proof that a work is unavailable.
- API credentials never enter the media-byte client; media URLs are host/path constrained and redirects are disabled.
- No upstream filesystem/download completion semantics may bypass command staging, generation checks, inventory gates, task revision checks, or `production_enabled=false`.

## Scaling implications

The 2026-09-12 batch queue review reopened all three pinned download implementations. Their bounded scheduling and all-image completion requirements remain references; desktop cross-work scheduling now wraps the existing verified single-work path and waits through PC registration before dispatching the next explicitly confirmed work. No source pins, media transport, retry policy, output layout or production flags change. See `BATCH_QUEUE_AND_MATCHING_2026-09-12.md`.

The live six-author JM+Pica validation established that real source traffic is substantial. For hundreds of authors, retain these upstream-inspired principles:

1. bounded batches rather than one giant scan;
2. checkpoint/resume for partial batches;
3. physical-request accounting including retries/failover;
4. source-specific error evidence, never inferred deletion;
5. pinned protocol/domain trust, with upstream updates reviewed before changing constants.

## Mandatory download-executor thaw reminder

Real local download execution is currently frozen. Before any future work enables or materially changes real JM/Pica download execution, staging-to-library promotion, replacement, completion mutation, or physical deletion, the current development session **must** read `AGENTS.md` and `docs/DOWNLOAD_EXECUTOR_THAW_GATE.md`, then re-open these three upstream repositories and compare the pinned download-related source again.

This requirement is intentionally duplicated across project-level files so a long time gap or a new conversation cannot safely proceed by relying on remembered protocol/download behavior alone.

The future re-review must cover at minimum JM scramble/image reconstruction, JM/Pica pagination, authentication/token handling, image path construction, retry/backoff, concurrency, temporary staging/finalization, partial-failure behavior, and each upstream project's definition of completion. Those upstream completion definitions remain references only; MangaMonitor-AI's stricter approval, task-revision, source identity, staging, manifest, filesystem verification, inventory, promotion, replacement, deletion, and production gates remain authoritative.

## Update policy

When one of the three upstream repositories changes protocol constants, auth headers, endpoints, pagination, image transforms, domain lists, or download/completion behavior:

1. record the new upstream commit here;
2. compare the exact source change;
3. add regression tests before updating adapter constants;
4. run workspace CI plus a live JM/Pica canary;
5. never loosen existing safety gates merely to match permissive upstream behavior.
