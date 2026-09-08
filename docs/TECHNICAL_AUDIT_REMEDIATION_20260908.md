# Technical audit remediation record — 2026-09-08

This record is based on the current `origin/main` at `ee653f2`, not only on the
audit baseline `3d023ef`. PR #38 was already merged as `56f0fe8`; its bootstrap
changes were retained. The changes below are local to branch
`audit-fixes-20260908` and were not pushed or merged.

## A01–A16 status

| ID | Current conclusion | Root cause / change | Regression evidence |
|---|---|---|---|
| A01 | Fixed, fail-closed | `--resume` now loads current authoritative input and compares its semantic author/decision/inventory context with the checkpoint. A mismatch returns `RESUME_AUTHORITY_MISMATCH` before any stage write. This is a refusal guard, not a current-state/checkpoint merge or recovery algorithm. | `resume_refuses_to_replace_changed_authority_and_keeps_checkpoint` |
| A02 | Fixed | Identity reanalysis no longer supersedes every source task before deciding the outcome. Stable source/work/target bindings retain pending status; ignored and genuinely changed bindings still follow their existing invalidation paths. | `unchanged_pending_task_survives_unrelated_context_migration`; Phase 3A task tests |
| A03 | Needs design | The formal monitor still has no trusted scope-certificate input for new-work evidence. No new evidence authority or product rule was invented. | Existing scope-certificate fail-closed tests remain passing |
| A04 | Fixed | Page sequence, duplicate progress, total/page/limit consistency, empty-result semantics, and terminal conditions are checked before `COMPLETE`/`last_full`. JM detail redirects remain valid only for a structurally valid single-record page. | `contradictory_pagination_is_incomplete_and_never_full`; existing pagination tests |
| A05 | Fixed, offline-verified | Source bytes and processed bytes now undergo bounded, descriptor-format decoding for GIF/WebP/PNG/JPEG. GIF/no-op bytes remain unchanged; truncation cannot reach a successful receipt. Decoder limits are 20,000×20,000 and 256 MiB allocation. | `truncated_supported_images_never_reach_processed_media`; live-media and isolated-staging suites |
| A06 | Fixed with explicit compatibility | Core documents require schema versions 1–3; inventory accepts the existing 1–8 range; legacy author/inventory files without markers remain shape-validated and preserved. Future versions, unknown decisions authority fields, and duplicate review IDs reject. | `state_loader_rejects_unknown_schema_authority_and_duplicate_review_ids`; round-trip tests |
| A07 | Needs design | Checkpoint replacement is atomic, but the checkpoint and exported documents are not yet one atomic generation. A manifest-generation migration needs a defined reader/writer compatibility policy. | Existing manifest and staging tests |
| A08 | Needs design | Wave buffering and checkpoint cost remain bounded by the current implementation’s existing budgets, but a durable streaming/compaction design is not introduced in this patch. | Existing request-budget and orchestration tests |
| A09 | Fixed | Source/direct failure event IDs include `scan_id`; replay within one scan remains idempotent while the same failure in a new scan is visible again. Business event IDs retain their existing idempotence semantics. | `source_errors_are_visible_per_scan_but_replay_idempotent_within_scan` |
| A10 | Fixed at the production entry point | The normal Phase 3B cycle no longer injects the fixed historical inventory repair overlay on every run. Bootstrap retains an explicit, audited one-time overlay input. | CI shell assertion that the production cycle has no fixed overlay; bootstrap tests retained |
| A11 | Fixed | Windows progress test uses a filesystem-safe hash suffix instead of RFC3339 text containing `:`; the test is now an explicit Windows CI step. | `phase3b_scan_progress` passed locally on Windows |
| A12 | Fixed | CSV cells whose first non-whitespace character is `=`, `+`, `-`, or `@` receive a leading apostrophe before CSV quoting. | `review_csv_neutralizes_formula_leading_values` |
| A13 | Fixed, offline-verified | JM and Pica metadata/error/template response bodies are read in chunks with an 8 MiB cap before JSON/text parsing. | adapter limit unit tests; no live source request run |
| A14 | Not code-resolvable here | Current GitHub UI showed no open PRs, PR #38 merged with four checks, and current main CI running. Branch-protection/check enforcement still requires repository administration and plan/permission support; no workflow change can claim server-enforced checks. | Live UI inspection recorded separately in task handoff; repository permissions not changed |
| A15 | Fixed in validation CI | `ai-ci` has a branch/PR concurrency group with cancellation and Cargo registry/git/target caching. Durable state publication keeps its existing non-cancelling concurrency. | Workflow diff/static assertions; remote post-change run not available without push |
| A16 | Needs design | Boundary duplication and module coupling remain a refactoring topic. The patch keeps the media validation boundary shared without combining the audit items into a broad rewrite. | Workspace compile/tests cover the touched boundaries |

## Verification boundaries

No real JM/Pica download, inventory mutation, replacement, deletion, deployment,
production enablement, push, or merge was performed. Media and metadata changes
were tested with synthetic bytes and isolated temporary directories only.

The fixed Rust toolchain is `1.98.1` and all local cargo commands use
`--locked`. The full pre-fix baseline had 374 passing tests and one known A11
Windows failure caused by the colon-containing temporary path. After the
changes, `cargo test --workspace --locked` passed all workspace tests on
Windows and `cargo clippy --workspace --all-targets --locked -- -D warnings`
passed. The targeted audit regression and Phase3B Windows tests also pass.
`audit.rs` was treated only as a defect reproducer; its defect assertions are
not used as regression proof.

## Local commits

- `cbb716b` — fix audit boundary validation and CI regression coverage
- `ebbadaf` — fix resume authority and scan state correctness
- `d4025e0` — bound image decoding and metadata responses
- `2cadbf3` — make inventory repair explicitly one-time

The final documentation/verification commit follows the final test run so its
hash identifies the complete record.
