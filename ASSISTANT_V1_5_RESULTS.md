# V1.5 Results — Read-only add-only inventory update candidate builder

## Accepted executable candidate

- Executable candidate head: `c798fd6ca103343b795f4bc72ee35fc1f4ce7626`
- Baseline CI: run `34113453222`
- Linux `rust-regression`: success
- Windows `windows-local-executor`: success
- Production gate remained closed throughout validation.

The results-document-only commit containing this file is not a replacement for the executable candidate above.

## Scope completed

V1.5 adds a read-only candidate-building stage after a successful V1.4 local inventory rescan. It does not write `monitor-state`, does not edit `inventory_index.json`, does not complete a task, does not replace/delete local content, and does not enable production.

The builder accepts the current durable `State` plus one V1.4 `LocalInventoryRescanReport` and emits a deterministic proposal with operation `ADD_NEW_WORK_ONLY`.

The builder fails closed unless all of the following remain true at candidate-build time:

1. the V1.4 report has the exact current schema and all sidecar/manifest/filesystem/observation verification flags are true;
2. every downstream authority in the V1.4 report remains false;
3. the V1.4 observation hash and `RESCAN_<hash>` identifier recompute exactly from the report bindings;
4. the current inventory is schema version 8, has a non-empty rules version, and `total_work_ids` exactly equals the current work array length;
5. current work IDs, local item IDs, and JM/Pica source mappings contain no duplicates;
6. the proposed `work_id` is completely absent from the current inventory;
7. the same exact JM/Pica source ID is not already mapped to another work;
8. the current pending task still matches the report's task/work/revision/target/source binding;
9. the task is still `pending`, action `download`, and `old_local_item_ids` is empty;
10. the deterministic task ID and executor command ID still recompute exactly;
11. the current catalog entry is active, still `PENDING`, still bound to the proposed work/source, and its analysis context still equals `State::context()`;
12. the stored matcher evidence is still current matcher version `PROVEN_NEW` with reason `COMPLETE_SCOPE_DISJOINT_EXPLICIT_INSTALLMENT` and uniqueness `PINNED_COMPLETE_SCOPE_ALL_WORKS_EXPLICITLY_DISJOINT`;
13. the deterministic new work ID is still `WORK_SRC_<hash(source_key)>`;
14. the canonical author and source title still bind exactly to the task target;
15. matcher identity has no unresolved issues and carries an explicit supported content type;
16. collection/extra semantics and non-null coverage are rejected in V1.5 rather than inferred;
17. a deterministic proposed local item ID is derived from the current inventory snapshot hash plus the exact V1.4 rescan/manifest evidence and is rejected on collision.

## Proposed inventory shape

The candidate contains one complete `proposed_work` using the existing inventory layout:

- exact deterministic `work_id`;
- one deterministic proposed `local_item_id`;
- one confirmed canonical author;
- the source-backed primary title;
- `normalized_key` generated with the repository's existing `rules_core::normalize_title` compatibility convention;
- matcher-proven fandom when present;
- one version object carrying explicit/unknown language, censorship, color, translation and sample evidence;
- explicit content type;
- exact one-sided JM or Pica source mapping.

UNKNOWN is preserved rather than coerced. In particular, when sample/preview evidence is unknown, `sample_or_preview` is serialized as JSON `null`, which existing `local_version` reads back as `None` rather than false.

The candidate also binds:

- current inventory snapshot hash;
- current state context hash;
- current matcher version;
- V1.4 rescan/observation IDs;
- command/task/work/revision/target/source bindings;
- source identity hash;
- current and proposed total work counts;
- deterministic `candidate_hash` and `INVENTORY_CANDIDATE_<hash>` ID.

## Candidate-only authority boundary

A successful V1.5 candidate is **not an authorization to apply it**.

The candidate hard-codes all of the following to false:

- `inventory_mutation_authorized`
- `task_completion_authorized`
- `promotion_authorized`
- `replacement_authorized`
- `physical_delete_authorized`
- `production_enablement_authorized`

No V1.5 code calls `persistence::save` or any other state writer. The local CLI loads state, reads a rescan report, builds the candidate, and prints JSON only.

## Local/cloud separation

- `mangamonitor-local-inventory-candidate` is a Windows/local product CLI.
- It refuses `GITHUB_ACTIONS=true`.
- CI keeps `contents: read` only.
- The Linux cloud-workflow guard now statically forbids invocation of the local executor, importer, rescan, or inventory-candidate module/CLI from production/cloud workflows.
- CI uses deterministic synthetic state/report fixtures only and performs no live JM/Pica media download.

## CI coverage

Run `34113453222` at executable head `c798fd6ca103343b795f4bc72ee35fc1f4ce7626` verified:

- full locked workspace tests;
- `assistant-view` build;
- Clippy with `-D warnings`;
- assistant read-only/offline guard;
- state-only publication guard;
- cloud/local isolation guard including the V1.5 candidate capability;
- production remains disabled;
- Windows local orchestration regression;
- Windows V1.3 import regression;
- Windows V1.4 rescan regression;
- Windows V1.5 candidate tests;
- deterministic repeated candidate output without inventory mutation;
- existing-work and existing-source-mapping fail-closed behavior;
- stale/upgrade/non-add-only task rejection;
- unsafe or tampered V1.4 report rejection;
- stale identity context and non-`PROVEN_NEW` evidence rejection;
- GitHub Actions runtime refusal for all local product CLIs;
- Windows build of executor, importer, rescan, and inventory-candidate executables.

A semantic review after the first green V1.5 candidate detected that M2's structural `Identity.normalized` is not the same contract as the existing inventory `normalized_key`. The final accepted candidate therefore uses `rules_core::normalize_title` for that legacy field and was fully revalidated on Linux and Windows afterward.

## Next narrow v1 stage

V1.6 should be an independently audited **inventory apply authorization gate**, not a direct continuation of candidate generation.

Before granting any inventory mutation authority, V1.6 should re-read current state, revalidate the V1.5 candidate hash and its exact V1.4 evidence bindings, verify the current task revision and add-only status again, verify the inventory snapshot has not changed, and independently revalidate the current matcher/new-work conclusion rather than treating the V1.5 candidate as an authorization token.

Even then, task completion should remain separable from the inventory write until the durable post-write inventory state is independently re-read and verified. Upgrade/replacement/deletion remain outside this add-only path.

`production_enabled` stays false. Original No-AI project remains untouched.
