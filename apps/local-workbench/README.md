# Local workbench interaction preview

This is the React/TypeScript frontend implementation for the [confirmed V1 design](../../docs/LOCAL_WORKBENCH_V1_DESIGN.md), currently using eight original fictional works and SVG covers, with explicit 100-record acceptance fixtures. The accepted design is being implemented in stages; the [2026-09-09 implementation record](../../docs/LOCAL_WORKBENCH_IMPLEMENTATION_2026-09-09.md) separates working UI from unfinished native and account integration.

## Try the flow

- The homepage shows only simulated local inventory, in recent-import and all-works sections. The local-booklist tab is a placeholder pending storage integration. Browse or search the cover library. Click a cover to open a separate detail page; return preserves search, filters and list scroll position.
- The narrow icon rail links the library, source-filtered favorites demonstration, discovery, queue, name-only author following, and settings. Cover density switches between 5/7/9; covers wrap into vertical rows and reduce columns in narrow windows.
- **设置 → 外观** supports A full-window dark background and B top fade (initial default), separate cover density, and a locally selected PNG/JPEG/WebP (up to 2 MiB). Save commits only the current page; a failed write keeps the draft. Resetting the background preserves density.
- **设置 → 下载与资源** stores a resource profile with only simultaneous works and global image requests. These preferences are not connected to the simulated or real scheduler yet; there are no ZIP packaging or image-processing controls.
- In discovery or source favorites, enter multi-selection (or select the full current eligible scope), then choose **下载并入库**. One dialog shows every selected work and proposed ZIP destination. One confirmation adds the works to the automatic simulated queue.
- Open **下载队列** to pause the queue or individual tasks, retry the failed example and simulate cloud disconnection. An imported item waiting for sync does not go through download/import again.
- **演示安全退出** freezes the simulated queue and opens a reopen screen. Reopen and browser refresh preserve the mock tasks and explicit pause choices when browser storage is available.
- **设置 → 网络与诊断 → 重置样例** restores the original queue examples without clearing appearance preferences. Unknown/corrupt stored demo data also falls back to these examples.

All simulated activity is visibly labeled. Queue state is saved under `mangamonitor.workbench.demo.v1`; validated appearance/resource preferences use the separate `mangamonitor.workbench.preferences.v1` browser key. The app only reads a background file explicitly selected by the user. It never reads credentials, contacts JM/Pica/GitHub, calls Rust/native commands, or reads/writes a real manga library. Authors and source labels are fictional display examples, not real monitoring results. No images or fonts are fetched from external services; selected backgrounds are never uploaded.

There is no native Tauri shell or reader yet. Real ZIP packaging/verification, task authority, import, safe native shutdown/recovery and cloud publication still need implementation and acceptance. Browser persistence is only a demonstration, not the native durability contract. The settings page records product choices; it does not change computer settings.

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

Both servers bind only to `127.0.0.1`. Do not open `index.html` using `file://`; assets and module routing require HTTP. This preview does not require any secrets, environment file or native backend.

## Validation

The `Local workbench UI` GitHub workflow installs the lockfile, runs Node state-machine and preference tests, type-checks/builds the frontend, and runs Chromium UI acceptance with one worker. It uploads the built `local-workbench-preview` artifact for lightweight local display and browser evidence on failure. Routine successful-screen captures and design image regeneration have been removed at the user's request. Existing Rust CI remains unchanged.

```sh
pnpm test
pnpm build
pnpm exec playwright install chromium
pnpm test:ui
```

Prefer GitHub Actions for these checks. Local builds and browser testing must follow the repository's resource budget; there is no reason to launch Rust compilation to inspect this sample.

## Explicit long-list acceptance fixtures

Open `/?fixture=ready-100` and search discovery for “示例作品” to exercise 100 distinct eligible records, including records outside the viewport. Open `/?fixture=owned-100` for long local/recent grids. These are labeled synthetic datasets; their demo queue storage is isolated from the normal eight-record sample. All cover images remain browser-lazy-loaded. Row virtualization and real paginated source adapters are still pending.

See [the active acceptance record](../../docs/LOCAL_WORKBENCH_ACCEPTANCE_2026-09-09.md) for verification status and remaining work. Successful CI alone is not visual acceptance against the external approved PNGs.
