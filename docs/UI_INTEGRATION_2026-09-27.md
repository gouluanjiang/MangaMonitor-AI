# Accepted UI migration

## Authority and scope

On 2026-09-27 the user accepted the isolated whole-app UI prototype, including
its second sidebar/motion review, and asked to continue. This batch migrates the
accepted design into the existing React/Tauri workbench. The prior prototype
acceptance is not acceptance of the resulting executable.

The layout uses a dark, restrained purple palette, readable compact spacing,
grouped icon-and-label navigation with manual collapse, page-local search,
visible actionable progress/errors and folded secondary explanations. Recent
updates, rankings and source search have direct sidebar entries. Existing
discovery controls remain available. The main scrolling container, virtual-grid
geometry and source paging/anchoring code remain in use.

Custom backgrounds, density, A/B background modes, native dialogs, download
confirmation, deterministic inventory, names-only follows, in-app reading and
independent reader windows retain their existing behavior. No prototype data or
simulated actions enter native branches. No changes to requests, identity,
author attribution, download execution or real private documents are authorized
by this UI batch.

## Implementation

- `UiSidebar.tsx` owns presentation and routes through existing App actions.
- `ui-theme.css` scopes the approved theme to `.app-shell.ui-refined`. Reader
  trees remain outside this scope.
- `ui-motion.ts` handles pointer/keyboard/reduced-motion policy and reversible
  sidebar clip/translation. The grid lays out once at the final width; cards and
  text are never scaled. Resize/interruption cancel only presentation effects.
- `ui-motion.css` uses the approved precomputed Motion curve, native customizable
  select pickers where available, and native details intrinsic-size transitions.
  Older WebViews retain their native controls. Keyboard actions are immediate;
  system/manual reduction removes movement. Lists and search results do not
  animate. The generated spring's MIT notice is retained beside the stylesheet.
- Existing source, update, ranking and library components receive layout-only
  changes. Detailed scope text is folded; counts, incomplete/error status,
  retries and source restrictions remain accessible.
- Optional `appearance.refinement` stores night/forest/dusk, shade 30–95,
  blur 0–16 and reduced motion through existing preview/save/revision handling.
  TypeScript and Rust validate the same bounds. Absent fields preserve legacy
  serialized preferences without eager writes; selecting a new preference is an
  explicit save. An older strict binary is not promised to read a file after new
  fields have been saved. No external private localStorage fallback is added.

Native select styling follows the browser's own focus, keyboard and viewport
placement behavior documented in [MDN's picker reference](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/::picker).

## Verification plan and status

Seven synthetic UI cases cover dedicated navigation, pointer/keyboard/reduced
sidebar interactions, page-local queries, all cover menu choices, deep scrolling
through 320 synthetic books while changing sidebar width, reversible appearance
drafts and narrow layouts. Existing reader, source, download, settings and
inventory suites remain enabled. Three additional TypeScript and three Rust
tests cover old-file compatibility, round-trip persistence and rejected shapes.

Local work is editing, formatter application and independent review only.
Formal checks/builds run once per necessary revision/target in the existing CI.
CI execution, visual review of built artifacts and verified Dev delivery are
pending as this implementation record is first committed. No real source
queries, scans, downloads, follow edits, library writes or account actions have
been performed. Production remains disabled; no installer or release is created.

## Remaining roadmap

1. Finish integration validation and deliver the Dev UI for user acceptance.
2. Formal release preparation after that acceptance.

A6 full-author acceptance remains explicitly deferred. Optional historical
enrichment and the frozen updater do not become part of this UI implementation.
