# Assistant A1 validation results

Status: **PASS**

## Scope

A1 defines and implements a deterministic, read-only assistant data contract on top of the imported MangaMonitor state model. It does not add AI calls, state mutation, source requests, production scanning, downloads, replacement, or deletion.

## Implemented

- `ASSISTANT_DATA_CONTRACT_V1.md` defines bounded assistant-facing views.
- `crates/cloud-monitor/src/assistant.rs` derives:
  - scan summary;
  - review backlog summary;
  - bounded review batches;
  - pending task summary;
  - collection summary.
- Review batches are capped at 100 records and include only inventory work IDs explicitly named by the deterministic candidate set.
- `assistant-view` is a read-only CLI over existing persisted state.
- Three regression tests verify deterministic output, bounded batches, rejected unbounded limits, CLI behavior, and byte-identical input state.

## Regression result

GitHub Actions run: `34036937645`

Validated commit: `f5b5664afc8e813db28da0a3f4bf9dfb15877385` plus the preceding assistant implementation through `6e642e5c738d7ba8406110899ff698ce17230235`.

Result:

- workspace tests: **177/177 PASS** (174 imported regressions + 3 assistant tests);
- assistant CLI build: PASS;
- clippy `-D warnings`: PASS;
- assistant runtime smoke under invalid HTTP/HTTPS/ALL proxies: PASS;
- input M3 fixture hashes before/after: identical;
- production gate check: `production_enabled=false` PASS.

The runtime smoke generated all five view types while network access was deliberately unavailable and did not mutate its source state.

## Preceding CI failures

Three preceding runs were useful infrastructure/compile diagnostics and are closed:

1. `34036433844`: dependency bootstrap was incorrectly placed behind invalid proxies, preventing Cargo from reaching crates.io. No assistant runtime behavior was exercised.
2. `34036641030`: Cargo exposed one Rust type-inference error in the new `reason_counts` map; fixed with an explicit `BTreeMap<String, usize>` type.
3. `34036752088`: all 177 workspace tests and the assistant CLI build passed; CI then failed only because the pinned minimal Rust toolchain had not installed the clippy component.

Run `34036937645` installs clippy explicitly and passes the complete A1 gate.

## Usage-budget guard

The AI CI push trigger is path-limited to Rust/Cargo code, `monitor-config.json`, and the AI CI workflow itself. Documentation-only commits do not consume a full regression run. Manual dispatch remains available.

## Safety conclusion

A1 does not change the imported production matcher or any source-side observation. It creates a read-only semantic interface suitable for a later ChatGPT layer while preserving the core rule:

> AI may be absent indefinitely; deterministic collection/state can continue and unresolved review can wait.

A1 is accepted as the foundation for A2 read-only assistant delivery.
