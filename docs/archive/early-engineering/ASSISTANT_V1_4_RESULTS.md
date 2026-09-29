# V1.4 Results — Read-only local inventory rescan and deterministic observation report

## Accepted executable candidate

- Executable candidate head: `0a5d97f3cb7982350bb374ead2a224b7aa2bc133`
- Baseline CI: run `34110669982`
- Linux `rust-regression`: success
- Windows `windows-local-executor`: success
- Production gate remained closed throughout validation.

The results-document-only commit containing this file is not a replacement for the executable candidate above.

## Scope completed

V1.4 adds a read-only verification layer after a completed V1.3 add-only import. It does not mutate `monitor-state`, `inventory_index.json`, pending tasks, existing library content, or production configuration.

The rescan accepts only a V1.3 `LocalLibraryImportReceipt` plus the user-selected local library root and then independently re-verifies the imported directory.

The verifier:

1. validates the V1.3 receipt schema, command/task/work/revision/source/target bindings and deterministic `mangamonitor-<command_id>` relative directory;
2. requires the receipt to represent a completed, destination-verified import with staging preserved and `inventory_rescan_required=true`;
3. rejects any receipt carrying inventory-mutation, task-completion, promotion, replacement, physical-delete, or production-enable authority;
4. requires the library root and imported directory to be real non-link/non-reparse directories and requires the imported directory to canonicalize as a direct child of the selected library root;
5. reads `_mangamonitor.json` only when it is a bounded regular file and requires its exact SHA-256 to equal the V1.3 receipt;
6. revalidates exact command/task/work/revision/target/source/source-work/action bindings inside the sidecar;
7. recomputes the target hash and requires the target source key to equal `<source>:<source_work_id>`;
8. requires the retained source-completion proof to remain bound to the pinned JM/Pica upstream commit and to carry no downstream authority;
9. reconstructs the existing staging-only `LocalExecutionPlan` and reuses `staging_manifest::validate` rather than inventing a new manifest protocol;
10. requires manifest hash, file count, total bytes and all authority flags to match the V1.3 receipt;
11. walks the imported filesystem tree and rejects unexpected directories, unexpected files, symlinks/reparse points, special files, missing files, duplicate paths, size drift or SHA-256 drift;
12. allows exactly the manifest artifacts plus `_mangamonitor.json`;
13. re-verifies the sidecar again at the end of the filesystem pass so sidecar drift during the scan cannot yield a stale successful observation;
14. emits a deterministic `RESCAN_<hash>` report without exposing the absolute local library path.

## Observation-only result

A successful V1.4 report means only that the imported V1.3 directory was observed to match its exact receipt, sidecar and manifest at rescan time.

The report explicitly keeps all of these false:

- `inventory_mutation_authorized`
- `task_completion_authorized`
- `promotion_authorized`
- `replacement_authorized`
- `physical_delete_authorized`
- `production_enablement_authorized`

V1.4 therefore does **not** create or modify an inventory row and does **not** mark the source task complete. A later independently audited gate must decide whether a current V1.4 observation can be converted into an inventory update candidate.

## Local/cloud separation

- `mangamonitor-local-rescan` is a Windows/local product CLI.
- It refuses `GITHUB_ACTIONS=true`.
- The baseline Linux guard statically verifies that cloud workflows do not invoke the local executor, importer, or rescan module/CLI.
- CI uses deterministic synthetic filesystem fixtures only; it performs no JM/Pica live download and stores no manga media in GitHub.

## CI coverage

Run `34110669982` at executable head `0a5d97f3cb7982350bb374ead2a224b7aa2bc133` verified:

- full locked workspace tests;
- `assistant-view` build;
- Clippy with `-D warnings`;
- assistant read-only/offline guard;
- assistant state-only publication guard;
- cloud-workflow/local-executor/importer/rescan isolation guard;
- production remains disabled;
- Windows local orchestration contract;
- Windows V1.3 importer/public gate regression tests;
- Windows V1.4 exact rescan tests;
- deterministic repeated rescan output;
- media drift and unexpected-file fail-closed behavior;
- forged authority and sidecar binding fail-closed behavior;
- unsafe relative directory and missing-sidecar fail-closed behavior;
- GitHub Actions runtime refusal for local executor/importer/rescan CLIs;
- Windows build of all three local executables.

The first V1.4 CI attempt failed only at compile/test construction because a test fixture helper was shadowed and one test-only import was in production scope. Those were corrected without weakening any lint or safety gate. The accepted candidate above is the first fully green executable head and also includes the final sidecar re-verification hardening.

## Next narrow v1 stage

V1.5 should remain split from state mutation. The safest next sub-stage is a **read-only inventory update candidate builder** that consumes a current V1.4 rescan report and the current inventory snapshot, then produces a deterministic proposed add-only inventory change without writing it.

The candidate builder must not infer replacement/deletion, must reject `upgrade`, must preserve the existing inventory schema, and must remain separate from any later explicit apply/task-completion gate.

`production_enabled` stays false. Real metadata-only JM/Pica acceptance tests remain required before production enablement is considered.
