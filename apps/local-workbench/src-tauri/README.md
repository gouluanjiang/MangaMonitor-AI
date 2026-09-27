# MangaMonitor native desktop

This separate Rust workspace hosts the React workbench in Tauri 2. It provides scoped account/source, PC-library, download, preference, diagnostic and reader commands. The current product behavior is described in the [user guide](../../../docs/USER_GUIDE.md), not the original shell-only notes.

## Stable data identity

Keep the application identifier `com.mangamonitor.workbench.preview`, the `workbench-preview-v1` document subdirectory, the existing Windows credential namespace and the internal executable name. Version/display-name changes alone must not create an empty private store or discard saved sessions.

Main-window and reader-window capabilities are separate. JavaScript does not receive a general filesystem, shell or arbitrary HTTP capability. Native pickers and source-qualified IDs bind the permitted operations. Reader windows use native managed creation; ordinary remote navigation and unmanaged webview downloads remain blocked.

Versioned documents use revision checks, interprocess locking and atomic replacement. Unreadable or unsupported data is retained and reported, not silently reset. New optional receipt/credential formats are not a promise that older binaries can read the same data.

## Build and verification

Use Node 24, pnpm 11.19.0, Rust 1.98.1, MSVC and WebView2. The dedicated Cargo.lock pins GUI dependencies. Formal suites and builds run only in the existing CI for the current revision/target.

Before a native build or desktop dev launch, install locked frontend dependencies, fetch the locked Windows native dependencies, then run `node scripts/collect-release-licenses.mjs` from the repository root. See the parent README for exact preparation commands. Repeat collection when lockfiles change; the generated resource directory is intentionally not committed.

The Windows workflow verifies offline modules, isolated Credential Manager storage, native command/window permissions, Clippy, a built executable and real WebView startup/restart with synthetic documents. Candidate packaging also bundles user/permission notices and verifies its per-user NSIS install lifecycle in disposable CI. Installer existence alone is not acceptance: review the final workflow result and artifact manifest.

The app remains unsigned unless a later distribution decision configures signing. If WebView2 is absent, NSIS may download the official runtime bootstrapper. Candidate installer validation does not grant permission to overwrite the user's app data, run media downloads or enable production. It does not implement an updater.

The icon is generated from the existing favicon by `scripts/generate_icon.py`. Earlier shell-only material is preserved in [README-HISTORY.md](README-HISTORY.md).
