# Pinned extraction provenance

- JM: https://github.com/lanyeeee/jmcomic-downloader/tree/f0cdd724af6892002f2fb7be883b88832cebe7e9
- Pica: https://github.com/lanyeeee/picacomic-downloader/tree/77c8b62ede42b3afc074506d092313816af8092d

Protocol headers, request signatures, API paths, JM AES response decoding and field mappings are adapted from the respective `src-tauri/src/{jm,pica}_client.rs`, `responses/`, `types/search_sort.rs` and JM `config.rs`. MIT notices are retained beside this file.

Phase 1A changes: explicit client config instead of AppHandle; no GUI; no download/image/export endpoints; sequential requests; random 1–3 second delay before each HTTP request; 30-second timeout; no retries/redirects hidden from the request counter; sanitized errors (no raw response bodies or auth headers). New Rust dependency versions are locked in Cargo.lock; they are not claimed to be the upstream Cargo.lock.

Pica metadata chapter pagination propagates errors instead of silently discarding failed pages. Neither GUI's task-creation API is exposed as download completion. No download function is implemented in this phase.
