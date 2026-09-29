# MangaMonitor desktop development shell

This standalone Cargo workspace wraps the local workbench in Tauri v2. It stores
preferences and source-qualified local booklists through `workbench-storage`.
Its private directory is the platform app-data directory for
`com.mangamonitor.workbench.preview`, followed by `workbench-preview-v1`.
It has no account, scan, downloader, subprocess, or production-executor commands.

All document initialization, reads, writes, and image validation run on blocking
workers. The window opens even when storage cannot be initialized. Failed opens
return a sanitized IPC error and are not cached; a later read retries after the
user repairs the path or its permissions. Successful storage instances stay alive
through each operation. An unavailable platform app-data path reports
`APP_DATA_UNAVAILABLE` through IPC and requires an application restart to resolve.

The single `main` capability grants five application commands. `build.rs`
registers these commands with Tauri's app manifest so the capability is enforced.
No generic filesystem, shell, HTTP, or dialog permission is exposed to JavaScript.
The Rust-only native image picker supplies the selected path to the storage
crate's bounded image validator; cancellation returns `null` and does not write.
The renderer writes the returned background only when the user saves preferences.
Top-level navigation is restricted to packaged assets and the exact development
server. New windows and webview downloads are rejected.

IPC contract:

- `read_preferences` / `read_booklists`: no arguments; return `{ revision, value }`.
- `write_preferences` / `write_booklists`: `{ expectedRevision, value }`; return
  the next `{ revision, value }`, or a stable `{ code }` error. A stale revision
  never overwrites the stored document.
- `choose_background`: no arguments; returns `null` or
  `{ backgroundImage, backgroundName }`. No arbitrary path is accepted.

Use Node 24, pnpm 11.19.0, Rust 1.98.1 and the Windows MSVC toolchain. From
`apps/local-workbench`, use `pnpm desktop:dev` or `pnpm desktop:build` after a
frozen dependency install. Keep this crate outside the root Cargo workspace:
headless Linux checks do not need GTK/WebKit. Both Cargo lockfiles must be
committed. The dedicated Windows workflow runs storage tests, mock-runtime IPC
tests using the real capabilities, Clippy, an NSIS build with `--locked`, and a
mandatory smoke test of the actual built application. The smoke uses the CI
runner's WebView2 Runtime, an isolated WebView profile and real IPC to save
preferences/booklists, then terminates and reopens the process to check persistence.
It runs only in disposable Windows GitHub CI, never against a user's application data.
For elevated runners, the smoke uses supported per-application HKLM WebView2
overrides because current runtimes ignore environment overrides at high integrity.
It refuses pre-existing values and removes its own values after the test.
The workflow retains its unsigned installer before smoke for diagnosis; artifact
existence alone is not a passed desktop check. It does not create a release.
The installer uses per-user installation and may download the WebView2 bootstrapper
when the runtime is absent.

The icon is a code-generated rendering of the existing workbench favicon.
Regenerate it with Python 3: `python scripts/generate_icon.py`.

Official API references:

- [Tauri capabilities](https://v2.tauri.app/security/capabilities/)
- [Application command manifest](https://docs.rs/tauri-build/2.6.3/tauri_build/struct.AppManifest.html)
- [Native dialog API](https://docs.rs/tauri-plugin-dialog/2.7.3/tauri_plugin_dialog/struct.FileDialogBuilder.html)
- [Mock IPC testing](https://docs.rs/tauri/2.11.5/tauri/test/index.html)
- [Windows installer](https://v2.tauri.app/distribute/windows-installer/)
