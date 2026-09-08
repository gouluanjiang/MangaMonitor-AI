# Local workbench interaction preview

This is the first React/TypeScript UI sample for the [confirmed V1 design](../../docs/LOCAL_WORKBENCH_V1_DESIGN.md). It is a browser preview of the proposed desktop frontend, with eight original fictional works and SVG covers.

## Try the flow

- Browse or search the cover library. Click a cover to open a separate detail page; return preserves search, filters and list scroll position.
- Select available works, then choose **下载并入库**. One dialog shows every selected work and proposed ZIP destination. One confirmation adds the works to the automatic simulated queue.
- Open **下载队列** to pause the queue or individual tasks, retry the failed example and simulate cloud disconnection. An imported item waiting for sync does not go through download/import again.
- **演示安全退出** freezes the simulated queue and opens a reopen screen. Reopen and browser refresh preserve the mock tasks and explicit pause choices when browser storage is available.
- **设置 → 重置样例** restores the original examples. Unknown/corrupt stored data also falls back to these examples.

All simulated activity is visibly labeled. Queue state is saved only under `mangamonitor.workbench.demo.v1` in this browser's localStorage. The app never reads credentials, contacts JM/Pica/GitHub, calls Rust/native commands, or reads/writes a real manga library. Authors and source labels are fictional display examples, not real monitoring results. No images or fonts are fetched from external services.

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

The `Local workbench UI` GitHub workflow installs the lockfile, runs the Node state-machine tests, type-checks/builds the frontend, and runs Chromium UI acceptance with one worker. It uploads the built `local-workbench-preview` artifact for lightweight local display and browser evidence on failure. Existing Rust CI remains unchanged.

```sh
pnpm test
pnpm build
pnpm exec playwright install chromium
pnpm test:ui
```

Prefer GitHub Actions for these checks. Local builds and browser testing must follow the repository's resource budget; there is no reason to launch Rust compilation to inspect this sample.
