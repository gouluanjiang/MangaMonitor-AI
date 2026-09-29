# MangaMonitor-AI development rules

## Scope and context

- For resumed work or scope decisions, use the latest section of `docs/DEVELOPMENT_HANDOFF.md` and its current roadmap. Read older milestones and topic documents only when relevant; historical status, prompts and code presence do not grant execution authority or prove acceptance.
- Preserve the deterministic rules and `UNKNOWN` / `REVIEW_REQUIRED` states. Follow the latest explicit user decisions; do not restart accepted work or expand a planning-only request into implementation.
- For UI changes, use the affected page in `../MANGAMONITOR_UI_BASELINE_2026-09-09.md`, subject to later accepted scope changes. Preserve names-only author rows and the removal of ZIP/image-processing settings. Compare affected running UI with its accepted reference; do not regenerate design previews unless requested.

## Execution boundaries

- Keep `production_enabled=false` until explicit production acceptance. Real media execution is local-only; the executor must refuse GitHub Actions runtime execution.
- The 2026-09-08 add-only staging approval and later desktop gates authorize only their recorded scopes. Real account actions, downloads, library/inventory/completion mutations, replacement and deletion require the applicable current authority; UI or implementation approval alone does not grant it.
- Preserve approval generation, task revision, source/target/file identity, isolated staging, manifest verification and separate promotion/replacement/delete gates. Do not overwrite existing destinations or treat source/auth/network failures as absence or deletion. Keep credentials and private account/library data out of repositories and ordinary logs.
- Before changing download behavior or authority, apply `docs/DOWNLOAD_EXECUTOR_THAW_GATE.md`. Reuse applicable recorded reviews; refresh affected evidence when pins, contracts, behavior or scope change. Upstream code is an implementation reference, never authority for state transitions. Preserve copied-code license notices and pinned trust/request accounting.

## Verification and resources

- Formal checks/builds for the current desktop work run in CI. Keep required checks; do not duplicate a suite locally and in CI for the same revision/target. Local editing, formatter edits, distinct diagnostics and review remain available. Batch documentation-only updates with the next necessary push.
- Choose verification for the changed behavior; expand or rerun only for new changes, failures or unresolved risks. Full workspace/cross-platform suites, release builds, installers and soak runs stay in GitHub Actions. Synthetic CI, live acceptance and production acceptance are separate evidence.
- If a local compilation/check is separately assigned, first measure RAM, CPU and free disk: at least 4 GiB available RAM, no sustained high CPU load, one shared pipeline, initially `CARGO_BUILD_JOBS=1` and `--test-threads=1`; an idle desktop with at least 6 GiB may use at most 2/2. Use below-normal priority where supported. Below 2 GiB, limit work to minimal file operations/existing binaries; pause heavy work for that threshold, CPU above 80% for about 30 seconds, or reported lag. Recheck before resuming, preserve unrelated processes, and avoid large system-drive caches. Cache moves/deletion need their own scope.
- The accepted JM performance path uses a 20-image lifecycle window and parallel CPU image processing (`docs/JM_UPSTREAM_PERFORMANCE_2026-09-12.md`); this download decision does not relax test duplication or file authority.
