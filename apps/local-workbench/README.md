# Local workbench development app

This is the React/TypeScript frontend implementation for the [confirmed V1 design](../../docs/LOCAL_WORKBENCH_V1_DESIGN.md), with native source-account pages and a separate eight-work simulated library/queue. The browser preview retains explicit 100-record synthetic acceptance fixtures. The accepted design is being implemented in stages; the [2026-09-09 implementation record](../../docs/LOCAL_WORKBENCH_IMPLEMENTATION_2026-09-09.md) records the first batch; the [0.3.2 cover report](../../docs/LOCAL_WORKBENCH_COVER_SESSION_2026-09-09.md) records current verification and user acceptance, while earlier reports remain historical.

## Try the flow

- The homepage shows only simulated local inventory, in recent-import and all-works sections. The local-booklist tab supports creating, renaming, archiving/restoring and organizing mixed JM/Pica references. A work can belong to multiple lists; removing membership leaves inventory and queue state intact. Browse or search the cover library. Click a cover to open a separate detail page; return preserves search, filters and list scroll position.
- The narrow icon rail links the library, favorites, discovery, queue, name-only author following, and settings. In the Windows app, favorites/discovery/following use the connected account; in the browser they remain demonstrations. Cover density switches between 5/7/9; covers wrap into vertical rows and reduce columns in narrow windows.
- **设置 → 外观** supports A full-window dark background and B top fade (initial default), separate cover density, and a locally selected PNG/JPEG/WebP (browser: up to 2 MiB; desktop native picker: up to 8 MiB). Save commits only the current page; a failed write keeps the draft. Resetting the background preserves density.
- **设置 → 下载与资源** stores a resource profile with only simultaneous works and global image requests. These preferences are not connected to the simulated or real scheduler yet; there are no ZIP packaging or image-processing controls.
- In library, discovery or source favorites, selection can organize any work into booklists. Download eligibility remains separately checked; review/owned/queued items cannot become new download tasks through booklist selection.
- In the browser demonstration, enter multi-selection (or select the full current eligible scope), then choose **下载并入库**. One dialog shows every selected work and proposed ZIP destination. One confirmation adds the works to the automatic simulated queue.
- Open **下载队列** to pause the queue or individual tasks, retry the failed example and simulate cloud disconnection. An imported item waiting for sync does not go through download/import again.
- **演示安全退出** freezes the simulated queue and opens a reopen screen. Reopen and browser refresh preserve the mock tasks and explicit pause choices when browser storage is available.
- **设置 → 网络与诊断 → 重置样例** restores the original queue examples without clearing appearance preferences. Unknown/corrupt stored demo data also falls back to these examples.

The local library and queue remain visibly labeled synthetic data. Native source pages have separate work DTOs and cannot enqueue simulated downloads. The browser uses separate keys for demo state, preferences and booklists; fixture keys are isolated. Desktop preferences/booklists use only native IPC and never fall back to browser storage after a native failure. The demo queue remains an independent simulation in both runtimes. Backgrounds are never uploaded. Native account settings support JM/Pica login, optional Windows Credential Manager session persistence, bounded metadata queries and individual favorite operations. Passwords are not saved. The user has accepted the 0.3.2 source flows listed in the current report; this is not exhaustive source/network coverage. Real manga-library operations remain unconnected.

The Windows Tauri 2 shell stores three versioned documents under `%APPDATA%`/com.mangamonitor.workbench.preview/workbench-preview-v1 (preferences.json, booklists.json and account-scoped following.json). Writes check revisions, lock across processes, sync a temporary file and atomically replace the document. Corrupt/unsupported documents are retained and rejected; an error does not authorize overwriting them. Re-reading after an external edit is explicit. Five document/picker and eight account/source scoped commands are exposed to the main window; JavaScript cannot supply arbitrary filesystem paths. The native picker validates PNG/JPEG/WebP content and embeds the selected image bytes in the preferences document, so moving the original image does not break a saved background.

This is a development shell, with native UI persistence implemented. The user has accepted both real source logins, remembered-session restart, the 0.3.2 live covers/session reuse, and the selected website favorite write; this batch has no remaining acceptance checks. See the [development handoff](../../docs/DEVELOPMENT_HANDOFF.md) for the user-reported scope. A reader, ZIP packaging/verification, task authority, import, safe native task shutdown/recovery and cloud publication remain unfinished in this interface. Resource preferences are not yet connected to a real scheduler.

Favorite metadata and read progress use a bounded account/folder/direction catalog cache. Cover images use only a run-scoped Blob cache: successful thumbnails survive scrolling, virtual-card unmounts and detail/settings visits, but closing the app releases them. No cover bytes are persisted by native commands. The cache retains at most 4096 images and 256 MiB of compressed Blob bytes plus estimated metadata; visible leases protect displayed URLs, and least-recently-used released images can be evicted at the limit. Logout/account changes revoke the old session's object URLs. Legacy disk cover slots are cleaned at startup only when the old bounded registry safely identifies them; catalogs, preferences, booklists and following are kept.

## Development

Use Node 24 and pnpm 11.19.0. From this directory:

```sh
pnpm install --frozen-lockfile
pnpm dev
```

For a production frontend build:

```sh
pnpm build
pnpm preview --port 4173 --strictPort
```

For the Windows development app (MSVC build tools and WebView2 required), run `pnpm desktop:dev`. CI uses `pnpm desktop:build` to create an unsigned NSIS development installer. The independent `src-tauri/Cargo.lock` pins the GUI dependencies; the root workspace stays headless. Prefer the CI artifact to local native compilation on a memory-constrained computer.

Both servers bind only to `127.0.0.1`. Do not open `index.html` using `file://`; assets and module routing require HTTP. This preview does not require any secrets, environment file or native backend.

## Validation

The `Local workbench UI` GitHub workflow installs the lockfile, runs Node state-machine and preference tests, type-checks/builds the frontend, and runs Chromium UI acceptance with one worker. It uploads the built `local-workbench-preview` artifact for lightweight local display and browser evidence on failure. Routine successful-screen captures and design image regeneration have been removed at the user's request. The baseline Rust workflow also tests source protocols, credential codecs/CAS, account transitions and document storage using synthetic fixtures. `Local workbench desktop` runs Windows document/account tests, a real credential roundtrip in random CI-only slots, native IPC/capability tests, Clippy, an NSIS build, and an actual WebView/process-restart smoke against the built executable. Browser tests and mock-runtime IPC tests are reported separately from the actual native smoke.

```sh
pnpm test
pnpm build
pnpm exec playwright install chromium
pnpm test:ui
```

Prefer GitHub Actions for these checks. Local builds and browser testing must follow the repository's resource budget; there is no reason to launch Rust compilation to inspect this sample.

## Explicit long-list acceptance fixtures

Open `/?fixture=ready-100` and search discovery for “示例作品” to exercise 100 distinct eligible records, including records outside the viewport. Open `/?fixture=owned-100` for long local/recent grids. Native runtime ignores fixture URL injection. These browser fixtures are labeled synthetic datasets; their demo queue storage is isolated from the normal eight-record sample. Source favorites render virtual rows at 5/7/9 columns and load the next source page near the viewport bottom. Offscreen image elements are released while their compressed thumbnails remain in the current run's bounded cache. Complete JM indexing starts only for an explicitly selected global reversal; Pica uses its native collection-time directions.

See [the second-batch report](../../docs/LOCAL_WORKBENCH_BATCH2_2026-09-09.md) and [the first-batch acceptance record](../../docs/LOCAL_WORKBENCH_ACCEPTANCE_2026-09-09.md) for verification status and remaining work. Successful CI alone is not visual acceptance against the external approved PNGs.
