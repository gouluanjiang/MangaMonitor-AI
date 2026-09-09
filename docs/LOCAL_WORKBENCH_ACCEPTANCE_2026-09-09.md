# Workbench continuation acceptance, 2026-09-09

## Scope and source evidence

This session preserves the existing React sample, Rust workspace and prior uncommitted design/settings implementation. Baseline checkout: codex/local-workbench-preview at 4e4ae36e82c01ba7ad5036e91cac8c41576c0765. PR #19 was an open draft; its old successful checks do not validate this batch.

The required UI baseline and source PNGs remain local in the parent workspace. The main agent opened B-banner-7.png, settings-appearance.png and settings-resources.png before editing, then A-dark-5.png, A-dark-9.png and A-same-crop-7.png during visual comparison. Author avatars/initial circles and image-processing/ZIP-tuning controls remain removed. No new design preview was generated and no private wallpaper or comic cover was added to the repository or CI.

## Implemented in this batch

- The library filters actual demo inventory and separates recent imports from the all-works/local-booklist tabs. The booklist tab remains an explicit placeholder.
- All cover lists share the vertical density grid and lazy cover loading. Ordinary browsing has no selection boxes; multi-selection and full eligible scope selection work for both source examples and discovery.
- Changes to search/source/status clear temporary selection and explain the changed scope. Density changes and settings/detail round trips retain the visible work anchor and list context.
- Settings use the accepted wide secondary navigation and open right-side form, with background selection before A/B previews, independent density and page-scoped save/reset. Only simultaneous works and shared image requests remain as resource controls.
- A temporarily undecodable saved background remains in recoverable preferences with explicit feedback. Saving another page cannot erase it. Settings search has its own query.
- 100 distinct synthetic records exercise actual application selection/state, replacing cloned DOM tests. Fixture queue storage is separate from the regular demo queue.

## Validation evidence

- Verified checkpoint head: aa8e62d6d09cf595f31fe3712424fa227cc27251. GitHub checked PR merge ref ad46025cd5aa9752eac0837100a435e7900211f3 against main 69fcf1dd24526b5920406c9b97bb3e8b8909c6c6.
- [UI run 34312348182](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/34312348182): 21 Node tests and 28 Chromium tests passed; formatting, TypeScript and production build passed. This includes both A/B modes at 5/7/9 densities with 100 ordered records, 390-pixel fallback, full-scope selection, isolated durable demo queue, settings context and clicked-work anchor recovery.
- [Baseline run 34312348149](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/34312348149): Linux 435 workspace tests plus Clippy and execution contracts passed; Windows 63 selected tests plus seven local CLI builds passed.
- Local single-process Node tests: 21 passed, about 0.4 seconds. Formatting uses the pinned pnpm 11.19.0 installation and Node 24. Available RAM fell below 2 GiB, so no local Rust or full frontend build/test pipeline was started. Manual inspection used only the loopback Vite service and the controlled browser.

The main agent inspected the running page at 1672 x 941 and 100% scale, including default B/7, the handoff's private wallpaper in B/7 and A/5, A/7, A/9, and both appearance/resource settings. The approved rail, title hierarchy, separate recent/all sections, cover proportions, broad settings navigation, background picker, page save controls and two resource controls are implemented. The last visual correction keeps the unscrolled toolbar transparent over A-mode wallpaper, and adds a dark readable surface only while sticky; an additional browser regression checks this transition. The 390 x 844 settings check found no horizontal overflow and retained the bottom navigation. No new design preview or private asset was committed.

This is acceptance of the listed first-batch UI surfaces and synthetic interactions, not all 20 page baselines or Windows V1. The complete 100-record layout and all six A/B-density combinations are behaviorally checked in Chromium; manual visual checks above are listed separately and do not imply a native desktop run.

Final toolbar refinement and documentation are committed after the verified checkpoint. Exact final-head checks and run links are recorded on [draft PR #19](https://github.com/gouluanjiang/MangaMonitor-AI/pull/19). The checkpoint results above must not be substituted for the PR's final-head checks when merging.

## Remaining delivery work

This is a browser fixture implementation, not Windows V1 delivery. Source-specific favorites/discovery/review/following/detail pages still require completion and individual baseline comparison. Native Tauri bridge, credential storage, actual JM then Pica sessions, booklists, library metadata and export, durable native queue, interruption/lost-receipt recovery, Issue #7 publication bridge and V1 end-to-end acceptance remain open. The browser background-size limit is not a final native product limit. Large-list row virtualization remains pending; current covers load lazily but record DOM is not virtualized.

No real account operation, download, media import, business-state publication or production enablement occurred. production_enabled=false and all execution gates remain in effect.
