# Workbench source accounts and metadata

This crate is independent of the cloud adapters and local manga executor. It
supports account login/session validation, one page of favorites or keyword
search, one work's details, a confirmed desired favorite state, and small cover
rasters. It cannot enumerate chapter images, download manga, create tasks, write
the inventory, or modify production state. Real requests refuse GitHub Actions.

## Public contract

DTOs live in `src/types.rs`. `WorkbenchSources::new()` constructs the clients.
All asynchronous methods return `SourceResult<T>`:

- `login(source, username, password) -> LoginResult`
- `restore(source, &StoredCredential) -> LoginResult`
- `profile(&SourceSession) -> SourceAccount`
- `favorites(&SourceSession, FavoritePageRequest) -> SourcePage`
- `search(&SourceSession, keyword, page) -> SourcePage`
- `detail(&SourceSession, id_or_link) -> SourceWork`
- `set_favorite(&SourceSession, work_id, desired) -> FavoriteUpdate`
- `thumbnail(&SourceSession, work_id) -> Option<String>` (static JPEG data URL)

`LoginResult` contains the opaque `session`, validated `account`, and `credential`.
The credential uses JM `SessionCookie` (the AVS value alone) or Pica `SessionToken`.
The caller chooses whether to store it in the credential vault. Passwords are
used only for the current sign-in request; they are never a stored credential.
`SourceSession` is not serializable and has no Debug implementation. A login or
restoration only succeeds after a profile request verifies the session.

The account DTO contains only source, account ID and display name. Work DTOs never
contain raw source JSON, cover URLs, user profiles, cookies, tokens or execution
authority. Optional booleans/counts retain unknown values as null.
Counts above JavaScript's maximum safe integer (9,007,199,254,740,991) are rejected.
Each serialized work is limited to 64 KiB, including JSON escaping. Titles are
limited to 2,000 UTF-16 code units, descriptions to 10,000, and each author/tag
to 2,000 with at most 64 entries in each array. Folder names have a 2,000-unit
limit. These string limits use the same units as JavaScript String.length.
JM author/tag arrays may contain blank string placeholders. They are omitted
only after checking the original array's 64-entry limit and every entry's string
type and 2,000-unit limit. Nonblank entries retain their order and exact content.
This compatibility rule does not apply to Pica or relax IDs, titles, favorite
state, counts, the whole-work budget, or cover transport validation.

All metadata responses are limited to 8 MiB and each request has a 30-second
timeout. Metadata redirects are rejected. There are no implicit retries or whole-list
pagination loops. JM requests use the pinned primary `www.cdnhth.cc`; this crate
does not rotate a logged-in account's cookie between domains. The five existing
cloud-domain pins are unchanged and may also identify pasted API detail links.

JM: POST `/login` uses form fields `username/password`; subsequent POST `/login`
uses `Cookie: AVS=<session>` with no password. GET `/favorite` uses page, `o=mr`,
folder_id (default 0), and preserves `folder_list` metadata. Returned folders
serialize as `{id, name, count}`. The pinned upstream folder schema has no
per-folder count, so `count` is explicitly null, never an invented zero. The
request still uses `FavoritePageRequest.folderId`. The API's POST
`/favorite` accepts `aid` and toggles add/remove; it does not select a folder.
Search uses GET `/search?main_tag=0&search_query=...&page=...&o=mr`; `redirect_aid`
is handled by an exact detail read. GET `/album?id=...` returns one work.

Pica: POST `auth/sign-in` returns the session token; GET `users/profile` validates
it. GET `users/favourite?s=dd&page=...` returns a paged `comics` object. Keyword
search uses POST `comics/advanced-search?page=...` with keyword/sort/categories.
GET `comics/{id}` returns details. The account-only supplementary reference below
defines POST `comics/{id}/favourite` with no body, returning action `favourite` or
`un_favourite`; details provide `isFavourite`.

`FavoritePageRequest.reverse` defaults to false. Pica uses source-side `dd`
(newest first) or `da` (oldest first). JM's default remains exactly `o=mr`;
reverse=true is rejected with SOURCE_REVERSE_UNSUPPORTED before a request.
The caller may implement a clearly scoped local reverse of a complete JM list.

Favorite writes first read the current state. An already-satisfied desired state
is a verified no-op. Otherwise exactly one toggle is sent, then one read-back
verifies the desired state. A timeout may be reconciled by that read-back, but
never causes another toggle. `FAVORITE_STATE_UNKNOWN` prevents writing when the
initial state is unknown; `FAVORITE_OUTCOME_UNKNOWN` reports an unconfirmed result.
The owning service must handle cancellation/session generations and pending
uncertain actions rather than treating this error as retry permission.

Known covers are indexed only from this session's validated metadata, with at
most 20,000 known work IDs, 128 recently used cover URLs, and no byte cache.
Unknown work IDs return WORK_NOT_LOADED. A known missing cover returns None.
An evicted URL retains its distinct state and is restored by one read-only
detail request under the session's metadata lock; failed recovery is an error,
never a false claim that the work has no cover. This grants no favorite authority.
No caller-supplied URL is accepted. A separate client carries no account headers
or cookies. It accepts at most 1 MiB and decodes at most 4 million
pixels/4096 per side/32 MiB allocation, and emits a static JPEG at most 512 per
side and 256 KiB (before base64 encoding). Original animations/active formats
are not passed to the renderer.
JM cover IDs map to `/media/albums/{id}_3x4.jpg` on exactly three pinned CDN
candidates: `cdn-msp.jmapiproxy1.cc`, `cdn-msp.jmapiproxy2.cc`, then the legacy
card host `cdn-msp3.18comic.vip`. The first two come from the already-pinned
Python client's cover generator and image-domain list. Each JM GET has a
10-second timeout inside the existing 30-second whole-operation timeout.
Only connection failures, timeouts, HTTP 404 or 502-504 may advance to the next
fixed candidate. HTTP 401/403/429, unsafe redirects, oversized data and decoding
failures stop immediately. A candidate is never retried in the same operation,
and JM redirects must preserve the exact work ID and `_3x4.jpg` path.
JM sends a fixed public browser User-Agent from that pin and advertises only
JPEG/PNG/WebP/GIF, which this crate decodes. It does not forward credentials or
Referer. Pica's headers and per-request timeout are unchanged. Pica accepts
only HTTPS `storage1.picacomic.com`, `s3.picacomic.com`,
`storage-b.picacomic.com`, and `img.picacomic.com`. Restricted static paths
support CDN transformation components such as `rs:fill` and `g:sm`.
Unknown hosts make `coverAvailable=false`.

The cover client disables automatic redirects. Redirects and fixed candidate
failover share at most four GETs total, including relative locations; no mirror
receives a fresh redirect budget. This also bounds redirects to at most three.
Every hop must remain HTTPS on the exact source-specific CDN allowlist and a
restricted image path. Credentials, query strings, fragments, nonstandard
ports, IP destinations, encoded or literal traversal, loops, and unknown origins
are rejected. Redirect bodies are not consumed. Recovery, all hops, and decoding
share one 30-second timeout. No account headers or Referer are forwarded.

ID parsing is local and never fetches a pasted URL. It accepts decimal JM IDs,
24-hex Pica IDs, pinned API detail URLs, and `18comic.vip/album/{id}` (with optional
www). Other domains, app-specific share formats and redirects are unsupported;
the user can supply the source ID instead.

No website author/series follow endpoint was established in the pinned sources.
Local follows must be labeled local. JM folder moves/creation, Pica favorite
folders, rankings, source comments/likes, registration, password recovery and
automatic session refresh are not implemented here. Neither site availability
nor real account behavior has been live-validated for this batch.

## Account-only protocol references

Existing download pins remain unchanged. These references were read only for
account/catalog protocol facts and thumbnail metadata, not download execution:

- JM original account API: [lanyeeee/jmcomic-downloader f0cdd724, jm_client.rs](https://github.com/lanyeeee/jmcomic-downloader/blob/f0cdd724af6892002f2fb7be883b88832cebe7e9/src-tauri/src/jm_client.rs#L162), login/profile L162-241, search/detail L249-328, favorites L400-440/L520-555.
- JM cookie and API-folder limitation: [JMComic-Crawler-Python 9fddb049, jm_client_impl.py](https://github.com/hect0x7/JMComic-Crawler-Python/blob/9fddb0494caf0cdc812ac6cbfc1c62f4f845b058/src/jmcomic/jm_client_impl.py#L867), AVS L899-901 and API folder limitation L962-978.
- JM covers: [same fixed ComicCard.vue L54](https://github.com/lanyeeee/jmcomic-downloader/blob/f0cdd724af6892002f2fb7be883b88832cebe7e9/src/components/ComicCard.vue#L54).
- JM cover-only fixed alternatives: [Python `jm_config.py` L184-192](https://github.com/hect0x7/JMComic-Crawler-Python/blob/9fddb0494caf0cdc812ac6cbfc1c62f4f845b058/src/jmcomic/jm_config.py#L184) lists the exact mobile CDN names; [the same pin's `jm_toolkit.py` L404-421](https://github.com/hect0x7/JMComic-Crawler-Python/blob/9fddb0494caf0cdc812ac6cbfc1c62f4f845b058/src/jmcomic/jm_toolkit.py#L404) generates `/media/albums/{id}{size}.jpg` using those domains. The User-Agent comes from `jm_config.py` L211-215; image credentials/Referer and dynamically discovered domains are not adopted.
- Legacy CDN failure evidence, not diagnosis of this user's connection: [upstream issue 232](https://github.com/lanyeeee/jmcomic-downloader/issues/232) reports connection error 10060 in August/September 2026; [issue 204 and maintainer discussion](https://github.com/lanyeeee/jmcomic-downloader/issues/204#issuecomment-4241004614) describe an HTML challenge dependent on proxy routing. No challenge handling or certificate bypass is implemented. These reports justify bounded alternate endpoints, but do not establish that this user's failure has the same cause or that any candidate is currently reachable.
- Pica original account API: [lanyeeee/picacomic-downloader 77c8b62e, pica_client.rs](https://github.com/lanyeeee/picacomic-downloader/blob/77c8b62ede42b3afc074506d092313816af8092d/src-tauri/src/pica_client.rs#L118), sign-in/profile/search/detail L118-258, favorite page L338-373; [cover descriptor rendering L53](https://github.com/lanyeeee/picacomic-downloader/blob/77c8b62ede42b3afc074506d092313816af8092d/src/components/ComicCard.vue#L53).
- Supplementary Pica favorite/account-only pin: [Miuzarte/PicaComic-go 25d20c875b69c94f7980fad8d8d5b06c7ef3d1cb](https://github.com/Miuzarte/PicaComic-go/blob/25d20c875b69c94f7980fad8d8d5b06c7ef3d1cb/PicaComic.go#L304), POST toggle L304-308; [types.go](https://github.com/Miuzarte/PicaComic-go/blob/25d20c875b69c94f7980fad8d8d5b06c7ef3d1cb/types.go#L115) isFavourite L115 and response action L205-206. This adds no download pin or source authority.
- Historical Pica thumbnail host examples: [2024baibai/PicaComic-Api 382586581cac128dddbf66d95485c326036cbfc2, README.MD L198](https://github.com/2024baibai/PicaComic-Api/blob/382586581cac128dddbf66d95485c326036cbfc2/README.MD#L198), `storage1.picacomic.com` in the thumb descriptor; L519 names `s3.picacomic.com`. These examples bound the cover allowlist; they do not establish current availability.
- The original Pica pin also uses [storage-b in AppContent.vue L109](https://github.com/lanyeeee/picacomic-downloader/blob/77c8b62ede42b3afc074506d092313816af8092d/src/AppContent.vue#L109). Its [favorite order enum L13–14](https://github.com/lanyeeee/picacomic-downloader/blob/77c8b62ede42b3afc074506d092313816af8092d/src-tauri/src/types/get_favorite_sort.rs#L13) establishes dd/da.
- Historical transformed-cover and redirect evidence: [tonquer/picacg-qt discussion 48](https://github.com/tonquer/picacg-qt/discussions/48), EIKO4O4's 2024-04-22 cover log. It records s3 `/static/tobeimg/.../rs:fill:300:400:0/g:sm/...jpg` redirecting to img, and `/static/tobs/...jpg` redirecting to storage-b. This is protocol compatibility evidence, not a claim that those historical requests succeeded or that every current cover uses that shape.

## Error handling

Only explicit HTTP/API 401 is `SESSION_EXPIRED`. Login HTTP 400/401 is
`LOGIN_REJECTED`. General 403 is `SOURCE_ACCESS_DENIED`; 429 is
`SOURCE_RATE_LIMITED`. Neither means the credential expired. Empty credentials
are `AUTH_REQUIRED`; invalid credential shape is `SOURCE_CREDENTIAL_INVALID`.
Network, timeout, schema, pagination and cover errors remain distinct stable
codes. Errors contain no response body, URL, email, secret or credential payload.
Cover HTTP failures are `SOURCE_COVER_ACCESS_DENIED` (401/403),
`SOURCE_COVER_RATE_LIMITED` (429), `SOURCE_COVER_SERVER_ERROR` (5xx), and
`SOURCE_COVER_NOT_FOUND` when all attempted candidates return 404. If a timeout
or connection/server failure is mixed with 404s, the earlier non-404 failure is
retained instead. Pica has one candidate apart from validated redirects.
These codes concern a cover only and never expire a session or prove that a
work is unavailable. Null means validated metadata provides no cover descriptor.

Offline unit tests use private scripted metadata responses under cfg(test).
Every unscripted test request, including a cover request, is refused before
network access. These are synthetic fixtures derived from the fixed protocol
references, not captured real-account responses or live acceptance evidence.

See THIRD_PARTY_NOTICES.md for preserved attribution.
