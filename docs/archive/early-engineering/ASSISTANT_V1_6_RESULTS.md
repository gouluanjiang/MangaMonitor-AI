# V1.6 Results — Inventory apply authorization gate

## Accepted executable candidate

- Base main: `190655695f1409b123b264f7e8a5d84c5093add1`
- Executable candidate head: `6c8717a4221cf0a0318e168a0053fbda8ddf894f`
- Baseline CI: run `34115693093`
- Linux `rust-regression`: success
- Windows `windows-local-executor`: success
- `production_enabled`: false

The results-document-only commit containing this file is not a replacement for the executable candidate above.

## Scope completed

V1.6 is an authorization-only gate for the narrow V1.5 add-new-work path. It does not write `inventory_index.json`, does not mutate any durable state, does not complete a task, and does not authorize replacement, deletion, promotion, or production enablement.

A successful V1.6 artifact authorizes only the following future transition:

- write target: `inventory_index.json`
- transition: `APPEND_ONE_WORK_AND_INCREMENT_TOTAL_ONLY`
- exact proposed work: the V1.5 `proposed_work`
- exact count transition: current `total_work_ids` to current + 1

No write is executed by V1.6 itself.

## Independent current-state revalidation

V1.6 does not treat a V1.5 candidate as an authorization token. Before issuing an authorization artifact it independently revalidates current durable state and evidence.

The gate requires all of the following:

1. the supplied V1.5 candidate has the exact supported schema and `ADD_NEW_WORK_ONLY` operation;
2. every authority flag in the V1.5 candidate is still false;
3. the V1.5 candidate hash and `INVENTORY_CANDIDATE_<hash>` ID recompute exactly from its canonical fields;
4. the supplied V1.4 report still binds the same rescan, observation, command, task, work, revision, target, source and source-work ID;
5. the V1.4 report still has complete sidecar, manifest, filesystem and inventory-observation verification and no downstream authority;
6. the current inventory hash exactly equals the V1.5 snapshot hash;
7. the current `State::context()` exactly equals the V1.5 state-context hash;
8. inventory schema, rules version and work count remain unchanged;
9. the proposed work ID, proposed local-item ID and exact JM/Pica source mapping are still absent from current inventory;
10. the proposed work still has exactly one local item, one bound version, one confirmed author, one primary title and the exact one-sided source mapping;
11. the current pending task still has the same task ID, revision, target hash and source binding;
12. the task is still `pending + download`, `old_local_item_ids` is empty, and coverage remains null for this narrow path;
13. the current catalog entry remains active, `PENDING`, current-context and current-matcher bound;
14. the stored `PROVEN_NEW` evidence still contains non-empty scope certificates bound to the exact current author and inventory hash;
15. V1.6 reruns `matcher_m2::decide` against the current state, current source record and those current scope certificates;
16. the fresh matcher result must again be exactly `PROVEN_NEW` with reason `COMPLETE_SCOPE_DISJOINT_EXPLICIT_INSTALLMENT` and uniqueness `PINNED_COMPLETE_SCOPE_ALL_WORKS_EXPLICITLY_DISJOINT`;
17. fresh work ID, author, source title, matcher rule, issue set and source-identity hash must match the V1.5 bindings;
18. the complete fresh matcher outcome hash must equal the currently stored matcher outcome hash;
19. after that independent validation, V1.6 rebuilds the complete V1.5 candidate from current state + current V1.4 report and requires byte-semantic equality with the supplied candidate.

Any mismatch fails closed and emits no authorization.

## Authorization artifact

The deterministic `InventoryApplyAuthorization` binds:

- authorization ID/hash;
- candidate ID/hash;
- exact inventory schema/rules/snapshot/context;
- current matcher version;
- current and proposed work counts;
- V1.4 rescan and observation IDs;
- command/task/work/revision/target/source bindings;
- proposed local-item ID;
- source-identity hash;
- exact proposed-work hash;
- exact write target and transition.

Only `inventory_mutation_authorized` is true. The following remain false:

- `task_completion_authorized`
- `promotion_authorized`
- `replacement_authorized`
- `physical_delete_authorized`
- `production_enablement_authorized`

The authorization artifact is evidence for a later executor gate; it is not itself a mutation.

## Local/cloud separation

- `mangamonitor-local-inventory-apply-gate` is Windows/local-only.
- It refuses `GITHUB_ACTIONS=true`.
- It loads current state and reads the V1.4/V1.5 JSON inputs, then prints authorization JSON to stdout.
- It contains no durable-state write path.
- GitHub Actions permissions remain `contents: read`.
- Cloud workflow guards explicitly forbid the V1.6 module and CLI in production/cloud workflows.

## CI coverage

Run `34115693093` at executable head `6c8717a4221cf0a0318e168a0053fbda8ddf894f` verified:

- full locked workspace tests;
- Clippy with `-D warnings`;
- existing read-only/offline and publication guards;
- cloud/local isolation including V1.6;
- production gate remains closed;
- Windows V1.3 import regression;
- Windows V1.4 rescan regression;
- Windows V1.5 candidate regression;
- Windows V1.6 authorization tests;
- deterministic repeated authorization with unchanged inventory;
- tampered-candidate rejection;
- changed-inventory rejection;
- changed-task and changed-rescan rejection;
- stale scope-certificate rejection;
- stored/fresh matcher disagreement rejection;
- GitHub Actions runtime refusal;
- Windows executable build.

The first V1.6 CI attempt exposed only an unused test-only import under `-D warnings`. The fix moved `serde_json::Value` into the test module; the follow-up commit changed only two import lines and the final executable head was fully revalidated.

## No mutation performed

V1.6 has not written a real inventory entry. No task has been completed by this stage. No replacement or deletion has been enabled.

## Next narrow stage: V1.7

V1.7 should be a local-only **single-file add-only inventory apply executor**. It must not simply call the existing broad `persistence::save`, because that helper writes the complete durable state set rather than the one file authorized here.

Before any write, V1.7 should reload current state and re-run the complete V1.6 authorization against the exact V1.4 report and V1.5 candidate, then require exact equality with the supplied V1.6 authorization artifact.

The executor should be limited to constructing exactly one post-state from the current `inventory_index.json` by appending the authorized `proposed_work` and setting `total_work_ids` to the authorized proposed count. It must not modify pending tasks, catalog, decisions, reviews, scan state, authors, latest state, local media, or any other durable file.

The write mechanism must be atomic or otherwise explicitly crash-safe and auditable, with no overwrite of unrelated files. After the write, V1.7 must re-read `inventory_index.json` from disk and verify the exact post-state before reporting inventory-apply success.

Task completion must remain a separate later gate even after a verified inventory write.

`production_enabled` remains false. Original No-AI project remains untouched.
