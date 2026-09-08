# V1.1 Results — Commit-Aware Assistant State Publication

## Status

V1.1 is implementation-complete on `assistant-v1-assistant-state-publication`.

Validated executable head before this results-only commit:

- `e0ae1aeffefca6d29c11c41e60728b39f7e75a64`
- baseline CI run: `34103300302`
- Workspace tests (`--locked`): success
- Build assistant view CLI: success
- Clippy with warnings denied: success
- Assistant runtime remains read-only/offline: success
- Assistant publication remains state-only: success
- Production gate remains closed: success

## Architecture boundary

V1.1 closes the publication gap left intentionally open by A3/A4/A5 without changing the MangaMonitor responsibility split.

GitHub-side assistant publication is **state publication only**. It does not:

- search JM or Pica;
- receive JM/Pica source credentials;
- download manga media;
- access local manga staging or the managed library;
- execute A6 download commands;
- authorize replacement or deletion;
- enable production.

Manga media download remains a local Windows executor responsibility.

## Supported state publications

The new manual workflow can publish exactly one of three deterministic assistant surfaces:

1. author registry changes -> `monitor-state/authors.json`;
2. bounded review decisions -> `monitor-state/decisions.json`;
3. recommendation/user-approval gates -> `monitor-state/assistant-task-gates.json`.

No arbitrary repository path is accepted.

## Current-generation replay verification

A3/A4/A5 remain staging-first. Before publication, `assistant_publication` replays the staged operation against the **current** monitor state using the operation parameters embedded in the staged audit.

Publication fails closed unless the newly regenerated:

- proposed payload;
- operation audit; and
- decision/task preview where applicable

are exactly equal to the staged files.

This means intervening changes to review evidence, inventory, pending task generation, target hash, or the current gate ledger invalidate stale staged operations.

## Exact staging scope

Each publication kind accepts an exact staging file set only:

- author: `authors.json`, `author-change.json`;
- decision: `decisions.json`, `decision-change.json`, `reanalyze-preview.json`;
- task gate: `assistant-task-gates.json`, `task-gate-change.json`, `executor-preview.json`.

Unexpected files, directories, symlinks, or non-UTF-8 filenames fail closed. Regression coverage explicitly verifies that a staged `manga.webp` file is rejected.

## Commit-aware workflow

`.github/workflows/assistant-state-publish.yml` is manual (`workflow_dispatch`) and requires an exact expected `main` SHA.

The workflow:

1. checks local and remote `main` equal the expected generation;
2. reconstructs the deterministic A3/A4/A5 staging output from current state;
3. runs the independent publication replay verifier;
4. permits exactly one canonical monitor-state JSON file to enter the Git index;
5. checks remote `main` again before commit;
6. creates a single scoped state commit;
7. checks remote `main` again after preparing the commit;
8. pushes only if the generation is still current.

A concurrent scan or other state publication therefore causes a fail-closed race rejection rather than an overwrite/force-push.

## CI architecture guard

Baseline CI now watches changes to the assistant publication workflow and statically rejects source/download coupling.

The workflow is required to remain free of:

- `PICA_TOKEN`;
- `PICA_EMAIL`;
- `PICA_PASSWORD`;
- `--live-source`;
- the Phase 3B production-cycle runner;
- JM/Pica adapter invocation.

`production_enabled=false` remains independently enforced.

## Regression coverage

V1.1 adds coverage for:

- current-state author publication verification;
- current-state decision publication verification;
- current-state task-gate publication verification;
- exact single-file monitor-state mutation authority;
- explicit absence of manga-file/download/production authority;
- stale review/source-evidence rejection;
- stale pending task revision rejection;
- unexpected staging-file rejection;
- all previous workspace and safety regressions.

## V1 boundary after V1.1

Assistant/user decisions can now cross the staging-to-live-state boundary through a deterministic commit-aware mechanism without turning GitHub Actions into a downloader.

The next v1 stage is local Windows executor orchestration: consume an exact A5-approved task locally, run the already-built A6 execution core into local staging, and produce a verified receipt. GitHub Actions must remain metadata/state-only throughout that stage.

## Mandatory pre-production acceptance

Production remains disabled. After the remaining v1 local-executor/library/report integration is complete, perform the previously agreed live acceptance sequence before formal enablement:

1. single-author JM/Pica metadata retrieval;
2. multi-author retrieval with author isolation, pagination and deterministic deduplication checks;
3. optional one-work local download E2E only after explicit user approval;
4. exact state-diff review;
5. only then consider `production_enabled=true`.
