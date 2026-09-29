# V1.3 Results — Verified add-only local library import

## Accepted executable candidate

- Executable candidate head: `e2fa546313d1332e44cb1080ea4f37084ad80813`
- Baseline CI: run `34108678954`
- Linux `rust-regression`: success
- Windows `windows-local-executor`: success
- Production gate remained closed throughout validation.

The results-document-only commit that contains this file is not treated as a replacement for the executable candidate above.

## Scope completed

V1.3 adds a Windows/local-only import boundary after the already verified V1.2/A6 execution report. It does **not** add manga-download or manga-library mutation capability to GitHub Actions.

The local execution report retains the exact proof bundle needed for an independent import check: the A6.5 source-completion/manifest proof, A6.4 real-filesystem verification result, and exact preflight binding. All downstream authority flags remain false.

The importer:

1. revalidates the current pending task generation and exact user approval;
2. accepts V1.3 only for current add-only `download` tasks whose `old_local_item_ids` is empty;
3. rejects `upgrade`/replacement inputs — those remain an A7 concern;
4. rebuilds the verified execution receipt and current receipt view from the retained proof bundle;
5. re-runs real staging filesystem verification before any library mutation;
6. requires staging/library roots to be real non-link directories and forbids overlap;
7. uses a deterministic opaque `mangamonitor-<command_id>` directory under the explicitly selected local library root instead of inventing a title-derived path;
8. atomically reserves that fresh destination with `create_dir`, so an existing file or directory is never replaced;
9. rechecks current task/approval before every artifact copy, creates every artifact with `create_new`, and re-verifies size/hash after each copy;
10. verifies the complete media tree, rechecks current task/approval again immediately before completion, then writes `_mangamonitor.json` with `create_new` as the completion marker and verifies the final tree;
11. deliberately retains verified staging after success;
12. deliberately retains partial destination output on failure, but without the completion sidecar, so a partial import cannot be mistaken for a completed V1.3 import.

The implementation module is crate-private and exposed only through `local_library_import_gate`; external callers cannot bypass the V1.3 download-only and Windows-only capability checks.

## Authority boundary

A successful V1.3 import still does **not** authorize or perform:

- GitHub-side manga-file operations;
- monitor-state or `inventory_index.json` mutation;
- pending-task completion;
- upgrade/replacement of an existing local manga;
- physical deletion;
- production enablement.

The import receipt explicitly keeps those authorities false and requires a later local inventory rescan/result-report step.

## Cloud/local separation

- `mangamonitor-local-import` refuses `GITHUB_ACTIONS=true`.
- Baseline CI statically verifies that cloud workflows do not invoke the local manga executor/importer path.
- Windows CI only runs deterministic offline tests and builds the local executables. It performs no JM/Pica live manga download.
- No raw manga images or archives are stored in GitHub.

## CI coverage

Run `34108678954` at executable head `e2fa546313d1332e44cb1080ea4f37084ad80813` verified:

- full locked workspace tests;
- `assistant-view` build;
- Clippy with `-D warnings`;
- assistant read-only/offline guard;
- assistant state-only publication guard;
- cloud-workflow/local-executor isolation guard;
- production remains disabled;
- Windows local execution orchestration contract;
- Windows V1.3 import implementation and public capability gate;
- mid-import approval revocation retains partial output without a completion marker;
- pre-existing destinations are never overwritten;
- GitHub Actions runtime refusal for both local executables;
- Windows build of `mangamonitor-local-executor` and `mangamonitor-local-import`.

The two structural lint expectations are narrow and local to the V1.3 importer boundary: one documents the deliberately retained validated local-plan binding in the validation bundle and one documents the explicit eight-argument security boundary needed to carry state, ledger, command, proof, roots, confirmation and reload callback. Crate-wide `-D warnings` remains enabled and passed.

## Next narrow v1 stage

V1.4 should implement **local inventory rescan + deterministic result reporting**. Import success must not itself become task completion. The local library must first be rescanned and the imported `_mangamonitor.json` plus actual files revalidated before any task-completion/state-report candidate can be produced.

Automatic upgrade/replacement and physical deletion remain deferred to A7/A8 and are not required for the add-only v1 launch path.

Before production enablement, keep `production_enabled=false` and complete the already-agreed real metadata-only acceptance tests: at least one single-author JM/Pica search and one multi-author search, checking pagination completeness, author isolation, deduplication, matcher behavior, and state diffs. Only after those tests pass should production enablement be considered.
