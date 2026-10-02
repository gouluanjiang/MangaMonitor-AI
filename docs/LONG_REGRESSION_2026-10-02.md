# Synthetic long regression — 2026-10-02

## Baseline and execution contract

Remote refs and PRs were refreshed on 2026-10-02. Main remains
`01274ab1fdc377f7d93acd905a0c687376b2add0` and lacks the delivered browsing
features. PR #24 is an open draft at
`0fdc8fc7c5dbdec8a60e9eb2c152c7c067558014`; PR #25 is an open draft at
`9afbb4b0470b3939b9f8f8e0848dcc1c3a8b3827`. The latter differs from
application/test revision `19b50a40d3453b7b35d7c2f6171c155df2bd1f46` only in
two delivery documents. Use `9afbb4b0470b3939b9f8f8e0848dcc1c3a8b3827` as the
application baseline. The initial checkout was clean; work is isolated on
`codex/long-regression-20261002`. No other branch is rewritten.

AGENTS.md, CONTRIBUTING.md, current handoff, author-workspace and browsing
follow-up contracts and all three current CI workflows were read. The user's
2026-10-02 instruction explicitly permits cloud-local long tests and builds,
overriding the repository's default CI-only location for these operations.
Windows-specific evidence still requires Windows CI. No production profile,
credentials, real source requests, real media or installation is used.

## Workload planned before measurement

Seed: `20261002`. Target: **240 minutes of active operations in one Chromium
process/page**, plus isolated native storage exercise. Dependency installation,
builds and fixture qualification do not count. Operations use production built
frontend code, bounded synthetic IPC and generated JPEGs; there are no real
JM/Pica endpoints. Native storage evidence is separate from mocked UI storage.

| Area | Long workload / evidence boundary |
| --- | --- |
| Author search | Retained random tabs; repeat names; close/switch; per-author identity assertions; delayed closed-tab replies; progressive first/complete times |
| Recent | JM/Pica/combined; overlapping pages and repeated tail; late retained history; wheel navigation and return position; failed refresh preserves results |
| Covers/detail/reader | Synthetic JPEGs; cache reuse; repeated opens/closes; chapter/mode changes and rapid navigation; active session and cover counters |
| Special/scan | Synthetic special unread persistence through failures; manual scan interleaved with other pages; real future-new-work user acceptance remains pending |
| History | Deliberate opens; deduplication/capacity observations; disable and clear; native capacity/concurrency/reopen checks separate from IPC mock |
| Storage | Native store in marked temporary roots; simultaneous visits, CAS, checkpoint changes, corruption refusal, killed temporary writer and reopen |
| Downloads/library | Synthetic fixtures and required native/Windows suites; no real execution; Windows recycle/open-location/install/shortcuts not inferred from Chromium |
| Scale | 24 library entries initially; 2,000 thereafter; stable finite source/author working set for warm-window comparisons |

Metric samples every 60 seconds include renderer JS heap, DOM nodes/listeners,
renderer CPU task time, runner memory/CPU, Linux descendant RSS/CPU ticks/threads/
FDs, boundary request/error/active counts, reader sessions, history length and
scenario counts. Summed RSS may double-count shared pages. IPC active counts
are not measurements of hidden Rust task queues. Native queues and Windows
WebView memory are unmeasured unless separate evidence explicitly supplies them.
Retry attempts count a request following a failure with the same synthetic IPC
key (manual and automatic combined); they are not wire-level source retries.
Cover faults are deterministic first-two 5xx responses for a subset of synthetic
identities, followed by success; source timeouts/outages/rate limits are injected
at the IPC boundary.

Compare warmed windows at the same catalog size and workload. Cache warming
alone is not a leak. Preserve failing assertions, operation log, seed, bounded
preceding operation trace, screenshot and boundary calls. Classify failures as
product, fixture or environment only after evidence review. Three repetitions
quarantine a scenario while unrelated scenarios continue; a quarantined scenario
is never a pass. Severe data safety faults stop that affected scenario at once.

## Reproduction and interruption

From `apps/local-workbench`, after the pinned dependency install and `pnpm build`:

```sh
node tests/soak/run.mjs --minutes=240 --seed=20261002 --chromium=/usr/bin/chromium --output=/workspace/mangamonitor-soak/baseline-ui
```

Omit `--chromium` in CI to use Playwright's pinned browser. A short fixture
qualification run uses its own output directory and is not long-test evidence.
The runner records the exact harness SHA, real browser version and start time
only after startup. `checkpoint.json`, `metrics.ndjson`, `operations.ndjson`,
`latency.ndjson`, failure artifacts and `summary.json` retain evidence outside
the repository. SIGINT/SIGTERM write an incomplete summary. Abrupt platform
loss leaves the last atomic checkpoint and flushed NDJSON operations.

After interruption, keep existing evidence and start a new output directory.
Record the new process as a separate continuous segment. Never combine two
processes to assert absence of a single-process leak. Missing target duration,
incomplete coverage or unclassified failures cannot be reported as passed.

Native reproduction, with pinned Rust 1.98.1 and locked dependencies:

```sh
CARGO_BUILD_JOBS=1 cargo +1.98.1 build --locked -p workbench-storage --example long_regression
target/debug/examples/long_regression /workspace/mangamonitor-soak/baseline-storage 14400 20261002
python3 scripts/summarize-long-regression.py /workspace/mangamonitor-soak/baseline-ui
```

The storage runner preserves marked roots on errors/panics, stops that affected
workload immediately, and logs each operation before execution. Its small/large
catalogs contain 32/20,000 source records; they are not real local archives.

Qualification is separate evidence: early harness versions used a wrong combined
source option, ambiguous retained-page labels, an offscreen library title, and
an incorrect assumption that retrying an already successful cover would issue
another request. These were corrected at the fixture/input layer. History's
successful native-reader side effect is explicitly modeled, and detail scenarios
wait for loaded content before returning. Those preparation failures do not prove
product defects and are not discarded from the external qualification logs.

## Result

Preparation in progress. No baseline long-test result is claimed yet.
