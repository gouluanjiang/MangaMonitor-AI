# Workbench continuation acceptance, 2026-09-09

## Scope and source evidence

This session preserves the existing React sample, Rust workspace and prior uncommitted design/settings implementation. Baseline checkout: codex/local-workbench-preview at 4e4ae36e82c01ba7ad5036e91cac8c41576c0765. PR #19 was an open draft; its old successful checks do not validate this batch.

The required UI baseline and source PNGs remain local in the parent workspace. The main agent opened B-banner-7.png, settings-appearance.png and settings-resources.png before editing. Author avatars/initial circles and image-processing/ZIP-tuning controls remain removed. No new design preview was generated and no private wallpaper or comic cover was added to the repository or CI.

## Implemented in this batch

- The library filters actual demo inventory and separates recent imports from the all-works/local-booklist tabs. The booklist tab remains an explicit placeholder.
- All cover lists share the vertical density grid and lazy cover loading. Ordinary browsing has no selection boxes; multi-selection and full eligible scope selection work for both source examples and discovery.
- Changes to search/source/status clear temporary selection and explain the changed scope. Density changes and settings/detail round trips retain the visible work anchor and list context.
- Settings use the accepted wide secondary navigation and open right-side form, with background selection before A/B previews, independent density and page-scoped save/reset. Only simultaneous works and shared image requests remain as resource controls.
- A temporarily undecodable saved background remains in recoverable preferences with explicit feedback. Saving another page cannot erase it. Settings search has its own query.
- 100 distinct synthetic records exercise actual application selection/state, replacing cloned DOM tests. Fixture queue storage is separate from the regular demo queue.

## Validation state

- Local single-process Node tests: 21 passed (9 demo state, 12 preferences), about 0.4 seconds. Formatting applied using the existing pinned pnpm 11.19.0 installation and Node 24.
- Main-agent real-page inspection began at 1672 x 941, 100% scale, default B/7. Homepage shell, hierarchy, local-only records and two sections inspected. Full A/B and settings visual checks remain in progress; this is not yet final visual acceptance.
- Current-commit GitHub Actions format/build/Chromium and Linux/Windows results: pending. Complete CI runs are preferred because available local RAM fell below 2 GiB; no local Rust or full frontend build was started.

## Remaining delivery work

This is a browser fixture implementation, not Windows V1 delivery. Source-specific favorites/discovery/review/following/detail pages still require completion and individual baseline comparison. Native Tauri bridge, credential storage, actual JM then Pica sessions, booklists, library metadata and export, durable native queue, interruption/lost-receipt recovery, Issue #7 publication bridge and V1 end-to-end acceptance remain open. The browser background-size limit is not a final native product limit. Large-list row virtualization remains pending; current covers load lazily but record DOM is not virtualized.

No real account operation, download, media import, business-state publication or production enablement occurred. production_enabled=false and all execution gates remain in effect.
