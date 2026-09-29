# MangaMonitor desktop workbench

The current React/Tauri desktop implements PC ZIP-library browsing, connected JM/Pica favorites, author search and manual followed-author discovery, recent feeds/rankings, a native download queue, in-app reading and independent reader windows. See the [user guide](../../docs/USER_GUIDE.md), [current handoff](../../docs/DEVELOPMENT_HANDOFF.md) and [1.0.1 release record](../../docs/RELEASE_1.0.1_2026-09-29.md).

The browser preview uses labeled synthetic fixtures. It does not provide real accounts, native filesystem access or real downloads. The Windows application uses scoped native IPC; native failures never silently switch to a demo implementation.

## Development

Use Node 24 and pnpm 11.19.0. From this directory:

```sh
pnpm install --frozen-lockfile
pnpm dev
```

Vite binds to localhost. Use HTTP rather than opening index.html directly. Windows native development also requires the pinned Rust 1.98.1 MSVC toolchain and WebView2. The separate Tauri Cargo lockfile is committed; native GUI dependencies do not enter the root headless workspace.

Native builds also require generated license resources. After installing frontend dependencies, from the repository root run the following preparation once, and again after a lockfile change. CI performs this before native compilation; do not duplicate its builds locally.

```sh
cargo fetch --locked --manifest-path apps/local-workbench/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc
node scripts/collect-release-licenses.mjs
```

The generated `src-tauri/release-resources/licenses` directory is ignored by Git. Missing license material fails candidate packaging instead of producing an empty notice bundle.

Formal tests/builds run in the existing CI under [AGENTS.md](../../AGENTS.md). The frontend workflow runs formatting, logic tests, TypeScript/Vite and Chromium tests. Windows CI runs storage/source/download tests, isolated credentials, native IPC tests, Clippy and actual WebView startup/restart/reader-window lifecycle checks. Stable and release-candidate packaging add isolated installer checks and a verifiable artifact manifest. These are not authorization to scan websites or download real media.

## Runtime contracts

- PC files and verified source-qualified registration determine library ownership; no fuzzy or cross-site title matching.
- Search pagination and incomplete scopes are visible. Manual followed-author checks retain old undiscovered/unowned candidates and reuse historical catalogs.
- Downloads require preparation and confirmation, isolated staging, media verification and separate library registration. Existing outputs are not overwritten.
- Cover bytes and reader image caches are bounded and run-scoped, rather than a permanent downloaded-cover collection.
- Reader windows have constrained IPC, independent pinning and shared process lifecycle; closing the main window can leave readers alive.
- Optional JM remembered login stores credentials in Windows Credential Manager and performs bounded recovery after explicit expiry. Pica retains session-only persistence. Passwords never enter frontend preference documents or repository fixtures.
- Preferences, catalogs, reader progress and verified local records remain in the existing identifier's private data directory. Do not rename that identifier or run an older binary against newly written documents.
- The in-app updater remains deferred. This work does not enable scheduled cloud production.

The accepted UI design, custom backgrounds and source/queue safety contracts remain in place. Synthetic screenshots and fixture data are distinct from live-source/user acceptance.

Earlier implementation notes are preserved in [README-HISTORY.md](README-HISTORY.md); their sample-only and cancelled phone/booklist descriptions are historical.
