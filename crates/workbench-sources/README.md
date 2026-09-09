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

All metadata responses are limited to 8 MiB and each request has a 30-second
timeout. Redirects are rejected. There are no implicit retries or whole-list
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

Favorite writes first read the current state. An already-satisfied desired state
is a verified no-op. Otherwise exactly one toggle is sent, then one read-back
verifies the desired state. A timeout may be reconciled by that read-back, but
never causes another toggle. `FAVORITE_STATE_UNKNOWN` prevents writing when the
initial state is unknown; `FAVORITE_OUTCOME_UNKNOWN` reports an unconfirmed result.
The owning service must handle cancellation/session generations and pending
uncertain actions rather than treating this error as retry permission.

Known covers are indexed only from this session's validated metadata, with at
most 1000 known work IDs, 128 cover URLs, and no byte cache. Unknown work IDs
return WORK_NOT_LOADED. Known works without a cover, or whose URL was evicted,
return None.
No caller-supplied URL is accepted. A separate client carries no account headers
or cookies. It accepts at most 1 MiB, rejects redirects, decodes at most 4 million
pixels/4096 per side/32 MiB allocation, and emits a static JPEG at most 512 per
side and 256 KiB (before base64 encoding). Original animations/active formats
are not passed to the renderer.
JM cover IDs map to the pinned card URL host `cdn-msp3.18comic.vip`. Pica accepts
only HTTPS `storage1.picacomic.com` or `s3.picacomic.com`, plus a restricted static
path from the metadata thumb. Unknown hosts make `coverAvailable=false`.

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
- Pica original account API: [lanyeeee/picacomic-downloader 77c8b62e, pica_client.rs](https://github.com/lanyeeee/picacomic-downloader/blob/77c8b62ede42b3afc074506d092313816af8092d/src-tauri/src/pica_client.rs#L118), sign-in/profile/search/detail L118-258, favorite page L338-373; [cover descriptor rendering L53](https://github.com/lanyeeee/picacomic-downloader/blob/77c8b62ede42b3afc074506d092313816af8092d/src/components/ComicCard.vue#L53).
- Supplementary Pica favorite/account-only pin: [Miuzarte/PicaComic-go 25d20c875b69c94f7980fad8d8d5b06c7ef3d1cb](https://github.com/Miuzarte/PicaComic-go/blob/25d20c875b69c94f7980fad8d8d5b06c7ef3d1cb/PicaComic.go#L304), POST toggle L304-308; [types.go](https://github.com/Miuzarte/PicaComic-go/blob/25d20c875b69c94f7980fad8d8d5b06c7ef3d1cb/types.go#L115) isFavourite L115 and response action L205-206. This adds no download pin or source authority.
- Historical Pica thumbnail host examples: [2024baibai/PicaComic-Api 382586581cac128dddbf66d95485c326036cbfc2, README.MD L198](https://github.com/2024baibai/PicaComic-Api/blob/382586581cac128dddbf66d95485c326036cbfc2/README.MD#L198), `storage1.picacomic.com` in the thumb descriptor; L519 names `s3.picacomic.com`. These examples bound the cover allowlist; they do not establish current availability.

## Error handling

Only explicit HTTP/API 401 is `SESSION_EXPIRED`. Login HTTP 400/401 is
`LOGIN_REJECTED`. General 403 is `SOURCE_ACCESS_DENIED`; 429 is
`SOURCE_RATE_LIMITED`. Neither means the credential expired. Empty credentials
are `AUTH_REQUIRED`; invalid credential shape is `SOURCE_CREDENTIAL_INVALID`.
Network, timeout, schema, pagination and cover errors remain distinct stable
codes. Errors contain no response body, URL, email, secret or credential payload.

Offline unit tests use private scripted metadata responses under cfg(test).
Every unscripted test request, including a cover request, is refused before
network access. These are synthetic fixtures derived from the fixed protocol
references, not captured real-account responses or live acceptance evidence.

See THIRD_PARTY_NOTICES.md for preserved attribution.
