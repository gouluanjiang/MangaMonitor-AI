# V1.2 Windows/local executor orchestration results

## Scope

V1.2 adds the first product-level local execution entrypoint for already-approved download commands. It composes the previously audited A5/A6 layers; it does not add a new downloader implementation and it is not a GitHub Actions production downloader.

Execution path:

`current approved ExecutorCommand -> current-generation revalidation -> live JM/Pica source preflight -> exact live media descriptors -> local media fetch/transform -> commands/<command_id> staging -> manifest/filesystem verification -> verified ExecutorReceipt -> ready_for_inventory_verification`

## Safety boundary

The local orchestration report keeps all downstream authorities closed:

- inventory mutation: false
- task completion: false
- promotion: false
- replacement: false
- physical deletion: false
- production enablement: false

Pica API credentials are used only for local metadata API calls. Media GETs continue to have no Pica credential parameter.

The standalone local executable refuses to run when `GITHUB_ACTIONS=true`. Baseline CI also checks that cloud workflows do not reference the local manga executor or live media execution path. Therefore GitHub Actions remains metadata/state-only.

The default completion timestamp is generated only after staging and verification succeed. An explicitly supplied timestamp must be valid RFC3339.

## Validation

Validated executable candidate head before this documentation-only commit:

`a491dde497f4d258076897b8e59741491ceffa2d`

Baseline CI run:

`34105426071`

Both jobs completed successfully.

Linux `rust-regression`:

- workspace tests: success
- assistant-view build: success
- Clippy with warnings denied: success
- assistant runtime remains read-only/offline: success
- assistant publication remains state-only: success
- cloud workflows never invoke local manga executor: success
- production gate remains closed: success

Windows `windows-local-executor`:

- local orchestration contract tests: success
- GitHub Actions runtime-refusal test: success
- Windows local executor build: success

No live JM/Pica manga download was executed by GitHub Actions during this validation.

## Next V1 stage

V1.3 should implement a local-only, add-only library import boundary for a verified V1.2 receipt/staging tree. It must fail closed on destination collision or state drift, must not overwrite existing files, and must not authorize automatic replacement or physical deletion. After import, local inventory must be rescanned before any task-completion/reporting transition is considered.

## Pre-production acceptance gate

Production remains disabled after V1.2. Before formal use, the project must still pass the agreed real-source acceptance sequence: single-author metadata search, multi-author metadata search, state/dedup/matcher review, and—only with an explicitly approved task—an optional local Windows download E2E. Formal production enablement is a later explicit step.