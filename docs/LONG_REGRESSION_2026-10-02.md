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

## Final result

The unchanged four-hour baseline and all scheduled supplemental sustained phases
completed. Three reproduced problems were fixed, and exact-candidate required
CI passed. Functional assertions and the specific closed-tab retention check
pass; whole-program resource growth is **not** declared completely explained
or leak-free. Local Windows and real future-notification acceptance remain.

All times below are **2026-10-02 UTC**. Durations use monotonic test clocks;
preparation, fixture generation, builds and final GC/snapshot capture are excluded
from pressure durations. Each operation is a scenario with several UI/API actions,
not one mouse click.

| Sustained segment | Start | End / process exit | Continuous duration | Completed operations |
| --- | --- | --- | --- | ---: |
| baseline-ui | 04:05:27.520 | 08:05:27.595 | 4:00:00.075 | 15,255 |
| baseline-storage | 04:01:55.809000 | 08:01:56.444000 | 4:00:00.634 | 50,597 |
| performance-before-pressure | 08:25:31.946 | 09:25:34.879 | 1:00:01.157 | 3,382 |
| performance-after-pressure | 09:41:51.734 | 10:41:54.676 | 1:00:01.030 | 3,428 |
| tab-lifetime-after-long | 08:35:45.972 | 09:35:47.674 | 1:00:01.700 | 2,054 |

The primary UI workload totals **6:00:02.262**
across three separate processes, **22,065 scenario operations**.
The longest primary UI process/page lasts **4:00:00.075**.
The parallel four-hour native run and one-hour tab diagnostic are not added to
that UI total, and separate processes do not prove one process was leak-free
for the accumulated duration. Performance process exits include a few seconds
of excluded final diagnostics; exact derived measurement endpoints are in
`final-duration-audit.json.gz`.

The baseline and candidate loading matrices each passed **40 trials / 400 actions**
(5 rounds × 2 catalog sizes × 4 background modes), outside the headline sustained
UI total. The closed-tab diagnostic completed **2,054 closures** and
**206 zero-retention checkpoints** across all/JM/Pica variants.
It forces GC every ten closures and retains its own growing identifier list;
its total heap/throughput trend is not a natural-app no-leak measurement.

### Scenario coverage and failure classification

| Original UI scenario | Passed operations |
| --- | ---: |
| tabs | 1,021 |
| library | 995 |
| fault | 1,025 |
| recent | 2,030 |
| history | 1,002 |
| reader | 1,989 |
| scan | 1,058 |
| author | 990 |
| recycleCancel | 1,060 |
| queue | 1,039 |
| late | 980 |
| detail | 1,027 |
| special | 1,039 |

All thirteen scenarios ran; zero functional failures, quarantined scenarios,
page exceptions or non-local network requests. The original baseline recorded
107,277 synthetic IPC calls, 2,136 boundary failures, 2,076 injected fault events
and 2,131 same-key post-failure retry attempts. These include intended failures
and cancellation races; they are not a 1.99% product-defect rate or wire-level
source retries. Maximum IPC/cover/query concurrency was 6/4/2; final active IPC
and reader sessions were zero.

The native baseline completed 50,597 real store operations: four concurrent
history writers (5,034), reopen/complete-catalog checks (5,089), discovery
patch/checkpoint updates (5,048), stale preference CAS (5,106), history disabled/
clear behavior (4,989), corruption/future/truncation/revision-zero refusal
(5,212), killed synthetic writer/reopen (4,949), browsing baseline scope/reopen
(5,145), shared library/download read consistency (5,058), and special/manual
marker separation (4,967). Main catalogs are 32/20,000 records; corruption and
kill-writer fixtures are always 32 records. Earlier checkpoint-03 grouping by
selected root size was corrected in the final baseline analysis.

Downloads/collision/partial-file/ENOSPC and Windows recycle details are below.
Untested items are explicitly listed in the Windows/platform boundary and
static-lead disposition tables. Data-safety faults were not observed. Fixture
qualification errors were preserved separately, including original locator/
retry assumptions, the diagnostic rAF capture race, and a rejected reused-Cargo-
target binary. No failing product assertion was deleted or loosened.

### Warm resource windows

Original baseline, medians; the catalog is 2,000 entries throughout these
windows. RSS sums include the test driver and descendants and can count shared
pages more than once. CPU is sampled process time divided by the measured
four-core cgroup quota; renderer main-thread CPU uses one core as 100%.

| Window (min) | JS heap MiB | Summed RSS MiB | Driver RSS MiB | CPU / 4-core quota | Main thread / one core | Author first / complete ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 30–60 | 30.75 | 2266.94 | 291.21 | 17.61% | 26.66% | 125.29 / 924.49 |
| 90–120 | 36.92 | 2622.03 | 333.08 | 17.93% | 27.41% | 121.31 / 920.72 |
| 150–180 | 39.50 | 2907.88 | 345.73 | 18.01% | 28.34% | 125.85 / 920.23 |
| 210–240 | 44.69 | 3159.69 | 372.58 | 18.35% | 28.57% | 125.52 / 921.49 |

Baseline descriptor median remained 493. Native storage RSS medians were
82.242 / 82.242 / 82.258 / 82.262 MiB in the same four windows, with six FDs and
one sampled parent thread; transient worker threads/children are not all
captured by minute sampling. Native quota-normalized CPU medians were
8.81 / 9.36 / 8.55 / 9.09%.

The original separate PSS observer was denied access; no permissions were
changed. Supplemental harnesses can read their own descendants and provide PSS:

| Version / window (min) | JS heap MiB | Summed RSS MiB | Summed PSS MiB | Driver RSS MiB | CPU / 4-core quota | FD median |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| baseline 0–20 | 54.65 | 2044.34 | 1695.55 | 176.16 | 14.37% | 493 |
| baseline 20–40 | 54.44 | 2187.17 | 1750.56 | 188.52 | 14.49% | 493 |
| baseline 40–60 | 57.38 | 2294.62 | 1865.75 | 197.71 | 13.91% | 493 |
| candidate 0–20 | 47.41 | 2054.67 | 1690.97 | 184.55 | 14.97% | 493 |
| candidate 20–40 | 53.22 | 2194.10 | 1881.22 | 198.95 | 14.95% | 493 |
| candidate 40–60 | 59.59 | 2263.04 | 1950.26 | 205.73 | 15.10% | 492 |

Each 20-minute cycle contains four five-minute background modes. The first
cycle includes warming. Baseline pressure overlapped the separate tab and native
diagnostics; candidate pressure overlapped the bounded reader-timing ABBA
diagnostic through 09:55:46 UTC. Concurrent browser processes also affect shared
page/PSS attribution. These resource windows are observational; the isolated
targeted ABBA probes establish the specific computation/commit speedups.

| Pressure run | Requests at final sample (includes setup) | Boundary failures | Max IPC / covers | Final active / reader sessions |
| --- | ---: | ---: | --- | --- |
| baseline | 26,364 | 30 | 7 / 4 | 0 / 0 |
| candidate | 26,686 | 45 | 7 / 4 | 0 / 0 |

Supplemental boundary failures are `reader_page` rejections in the fixture
when close has already removed the session; no functional assertion failed.
Native worker queue lengths and real source/network retry rates remain
unmeasured. Active IPC counts are not a substitute for those metrics.

| Run | Renderer JS heap after GC before / after pressure (MiB) |
| --- | ---: |
| baseline | 9.26 / 19.00 |
| candidate | 9.25 / 17.07 |

GC occurs outside the timed interval. Browser heap snapshots independently
show the baseline author-scope strings retained through a module Map. They also
show growing `blink::NetworkResourcesData::ResourceData` owned through
`NetworkResourcesData` by `InspectorNetworkAgent` / `DevToolsSession`. This is
a proven instrumentation ownership path, not attribution of all native/RSS
growth. Shallow sizes are not retained/dominator sizes. The measured tab fix
addresses its exact application retention path; warming, browser instrumentation,
native image/cache allocations and remaining unknowns prevent a blanket
whole-program no-leak conclusion.

### Full loading matrix

[The full paired matrix](../evidence/long-regression-2026-10-02/final-followup/performance-matrix-comparison.csv)
contains 80 exact size/background/scenario/phase rows with five samples per
version, first/complete median and p95, and request deltas. P95 is the maximum
at this sample count. Cold means a fresh Chromium/app cache; host OS caches
were not flushed. Startup excludes browser launch (logged separately); synthetic fixture initialization is included in that row and also logged separately. List
completion includes all requested metadata and visible covers, not offscreen
images. Recent completion means usable first rows/visible covers; repeated
sixth-tail termination is separately asserted. Reader first time is the shell;
complete time is a decoded visible synthetic page.

Mock background scan/download modes measure frontend IPC/progress/render
interference, not actual native executor CPU/I/O throughput. Actual native list,
cover, ZIP-page and local commit timings are reported separately. The bounded
image working set is 256 cover and 256 page JPEGs; source identity remains
source + work ID. Full matrices retain the original polling-based timestamps;
the reader outlier diagnosis below explains their observation precision.

Representative 2,000-entry idle matrix medians (ms; all other sizes/backgrounds are in the CSV):

| Scenario / phase | Before first / complete | After first / complete |
| --- | ---: | ---: |
| author / cold | 143.50 / 958.25 | 144.74 / 953.26 |
| author / warm | 54.11 / 66.97 | 53.83 / 65.11 |
| detail / cold | 311.08 / 403.29 | 315.96 / 405.51 |
| detail / warm | 301.54 / 400.28 | 309.90 / 401.95 |
| library / warm | 85.32 / 91.07 | 87.70 / 95.68 |
| reader / cold | 290.08 / 634.31 | 311.14 / 669.62 |
| reader / warm | 302.39 / 629.18 | 306.49 / 629.78 |
| recent / cold | 166.75 / 527.80 | 158.62 / 520.44 |
| recent / warm | 101.49 / 102.71 | 101.52 / 107.36 |
| startup-and-first-library / cold-process | 499.18 / 868.14 | 517.55 / 876.37 |

General loading is comparable within this descriptive sample; the reader polling caveat below applies. No across-the-board startup/network improvement is claimed.

## Confirmed issues and minimal changes

The original four-hour baseline finished before any application change. Its
functional assertions passed; resource trends and the user's static leads then
guided three repeatable diagnostics. A passing scenario loop did not conceal
these independently reproduced defects. No application changes were made for
the remaining unproven leads.

| Confirmed problem | Failure evidence | Minimal change | Validation |
| --- | --- | --- | --- |
| Closed author tabs retain complete position-key snapshots | Production diagnostic: 40 closed tabs retain 40 scopes / 7,200 keys after GC; retained keys rise at every ten-tab checkpoint | Track scopes owned by an ephemeral author tab; delete them after its final layout-unmount save. Hidden/open tabs and ordinary page positions retain their previous behavior | Same 40-tab reproduction retains zero; logic test protects neighboring scopes; CI tests all three source variants; 2,054 sustained closures retain zero closed positions |
| Unrelated source selection renders repeatedly filter/sort an unchanged catalog | 20 enter-selection/cancel pairs on 2,000 works invoke the title comparator 760,673 times; the zero-redundant-comparison assertion fails on the baseline | Memoize derived lists against actual data/query/sort/inventory inputs, including the existing explicit-content-evidence revision | Identical production probe performs zero comparisons and preserves displayed order; full label/filter and browser suites pass |
| Observation commits repeatedly parse unchanged local bytes | Release 20,000-record probe exposes about 99 ms per detail commit; cache-contract test fails with `unchanged observed-works.json was parsed again` | Add one observation-document slot to the existing exact-byte validated cache | Same-revision changed bytes invalidate; corrupt/future data is preserved and rejected; stale CAS rejects; four ABBA runs end with byte-identical observation documents |

The third change retains the existing file read, file identity checks, schema
and validation rules, CAS, atomic writes and limits (64 MiB serialized document,
100,000 observed works, 20,000 recent-history items). It caches a single current
raw document and parsed snapshot per store cache, not an entry per request or
author. It does **not** make observations asynchronous or weaken freshness.
The original whole-file write cost remains.

### Comparable targeted performance

All times below are milliseconds. Both targeted comparisons use ABBA order,
the same synthetic input and the same diagnostic code. Source-render samples
use the same Chromium build; observation samples use Rust 1.98.1 release builds
in distinct target directories. The native final documents are byte-identical.

| Probe | Samples per version | Before median / p95 | After median / p95 |
| --- | ---: | ---: | ---: |
| Source selection pair, 2,000 works | 40 | 394.68 / 441.38 | 330.02 / 378.46 |
| Observation fresh read, 20,000 records | 20 | 35.06 / 48.10 | 36.14 / 52.99 |
| Observation retained handle, changed file | 20 | 34.96 / 41.44 | 38.74 / 48.49 |
| Observation warm unchanged-file read | 20 | 34.30 / 44.59 | 4.44 / 8.50 |
| Observation merge of 30-record page | 20 | 99.49 / 132.61 | 70.30 / 88.48 |
| Observation merge of one detail | 20 | 99.17 / 117.49 | 66.38 / 93.23 |
| Observation 32-record warm read | 20 | 0.090 / 0.127 | 0.039 / 0.043 |
| Observation 32-record page commit | 20 | 1.459 / 3.997 | 0.950 / 2.573 |

The specific improvements are about 16.4% for the selection interaction, 29.3%
for the large page commit and 33.1% for the large detail commit. Cold reads did
not improve. These are local computation/commit measurements, not real-network
speedups. The source comparator call count falls from 1,521,346 to zero across
the two measured baseline/after runs per version.

Caching has a measured memory cost: the whole native diagnostic's peak RSS rose
from 111,512 to 178,904 KiB (about 109 to 175 MiB); user CPU fell from 3.387 to
2.572 seconds and system CPU rose from 0.139 to 0.220 seconds. These are fresh
process peak/CPU measurements, not isolated allocation sizes or evidence of a
long-term leak. No lower whole-program memory claim is made.

### Reader timing outlier classification

The original five-round matrix showed two 2,000-entry warm-reader median shifts:
418.54→663.96 ms with both background modes, and 439.50→642.89 ms with download
progress. The complete-time assertion uses Playwright polling; raw samples
cluster about 105 or 360 ms after the reader-shell observation.

A separate ABBA diagnostic kept every original assertion and added passive
animation-frame timestamps for a decoded visible image. Ten warm samples per
condition/version measured **435.95→436.00 ms** (download) and
**433.95→433.70 ms** (both). The original poll observation lag is about
236–246 ms at the median. The apparent 200–250 ms readiness regression did not
reproduce with that observer; no application change was made for this signal.
The original matrix remains archived. A first diagnostic-qualification failure
(reading the passive observer before its next frame) and its script are also
preserved; qualification and final ABBA results are separate. Frame precision,
small sample counts and concurrent pressure limit the diagnostic; it does not
prove invariant reader latency on all platforms.

### Native loading and recovery

Direct release diagnostics generated 24 and 2,000 ZIP entries, each with two
chapters and three generated JPEG pages, and passed 172 checks. Catalog and
archive bytes stayed unchanged. Small/large medians: warm list 0.042/0.862 ms;
cover load 14.99/16.64 ms; reader open plus first page 0.294/1.036 ms; page read
0.116/0.310 ms. Initial generated-library scans took 4.78/165.63 ms (one sample
at each size). The largest measured page read was 0.799 ms. This does not
establish per-page ZIP indexing as this batch's bottleneck; no reader-index
rewrite was made. These direct Linux timings exclude Windows IPC/WebView.

Five additional Linux ENOSPC tests passed locally and in exact-candidate CI.
They inject write failures only under fresh marked synthetic roots: ZIP and
directory zero/partial writes, and partial preference atomic writes. Five
native fault events plus the injector scope self-check produce six trace entries.
Previous valid data, library/download ledgers and an outside-injection sentinel
are preserved. Locks release; explicit retry after clearing
the injected fault succeeds with a verified receipt. No actual disk filling or
system/security configuration change is used. The ordinary suite deliberately
ignores these armed tests; the separate guarded CI step explicitly executes all
five and fails if any are omitted.

Existing required native suites also cover cancellation/drain, exact-prefix
partial-file resume, destination/name collisions, revision-bound queue changes,
corrupt/future documents, in-memory legacy migration until an explicit write,
and preservation of old state after partial source failures. The long storage
run exercises real store APIs with parallel writers and killed temporary writer
children. Corruption recovery restores the fixture's saved known-good backup;
this is **not** a claim that the product automatically repairs arbitrary damage.

### Static leads resolved, not established, and deferred

| Lead | Disposition in this batch |
| --- | --- |
| Closed author-tab browse positions | Confirmed and fixed; other accounts' retained maps are not inferred from this fix |
| SourceWorkbench repeated filtering/sorting | Confirmed and fixed only for measured unchanged-input renders |
| Observation duplicate local parse/commit cost | Confirmed and fixed only through the bounded existing-cache mechanism |
| Reader stale in-flight pages | Actual cache probe: 1,000 desired-window changes, maximum two active promises, no stale publication, zero active/entries after close. Old started requests still occupy slots; the controlled jump takes 147.83 ms with 80 ms promises. Not a native network benchmark; finer cancellation remains deferred |
| Detail per-call GET / request coalescing | No long-run backlog or cross-author overwrite reproduced at the synthetic boundary; native per-detail cancellation/coalescing remains unverified and unchanged |
| Per-page ZIP indexing | Generated native probe did not establish a meaningful bottleneck at the tested sizes; defer |
| Invalid-account savedViews / recent readers/views | Static retention paths remain; no account-churn RSS attribution or fix claimed |
| useBrowsingMarkers on progress; collection used_at full writes; preference background decode/write | Not independently established as this run's dominant cost; unchanged |
| Account lock during ranking; synchronous Windows Vault wait; joined account restoration; online metadata non-LRU eviction | Need applicable native/account evidence; unchanged |
| Download inventory duplicate verification | 24,683 mocked inventory reads observed in the baseline; this measures boundary traffic, not native full-file verification cost. Singleflight deferred |
| Diagnostic missing-code whitelist; inactive old cloud monitoring | Deferred outside the measured fixes; not declared harmless or resolved |
| Whole-browser RSS growth | Retained tab data is proven; DevTools-owned resource records are also present. Total RSS/PSS attribution remains incomplete; no blanket no-leak acceptance |

Per-launch fixed browsing badges, special-follow unread state, successful
manual-scan additions, source + work-ID identity, history cap 100 and current
explicit-tag filtering remain distinct and unchanged. Unknown tags remain
visible; there is no title/author/cover inference or extra filtering detail
request. Recycle confirmation and fail-closed Windows recycle semantics remain.

## Exact candidate, CI and Windows acceptance boundary

Application candidate: `1cba4b81eaf439de9d00c47a42aeb11027b1e1fa` on draft
[PR #26](https://github.com/gouluanjiang/MangaMonitor-AI/pull/26), stacked on
`codex/library-browse-followups`. Later evidence/document commits do not change
this application or its candidate artifact. Nothing was merged, released or
installed into the user's profile.

| Required check on that exact SHA | Result |
| --- | --- |
| [Baseline / Linux + Windows executor](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/36986184293) | Passed. Linux log: 1,094 Rust test executions, plus five explicitly armed ENOSPC tests; Windows executor: 63. Counts are executions, not distinct cross-job tests |
| [Frontend](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/36986184319) | Passed: 356 logic, 268 browser cases, production build/format, added tab-lifetime and unchanged-catalog render probes |
| [Windows desktop](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/36986184306) | Passed: 659 Rust test executions across job stages (including 57 native IPC), Clippy/format, 23 installer/license guards, native WebView/restart and isolated silent installer lifecycle |

The Linux cover profiling test remains outside the ordinary suite. Windows
credential-roundtrip tests use only random disposable CI slots and run in their
explicit step; no user credentials were accessed. No ignored armed fault test
is represented as having passed the ordinary suite.

Verified [Windows candidate artifact](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/36986184306/artifacts/11218078107),
name `mangamonitor-windows-candidate-1cba4b81eaf439de9d00c47a42aeb11027b1e1fa`:

| Item | SHA-256 |
| --- | --- |
| Artifact ZIP | `0990b87b39892c48cd2fcdf59f767ab3bd29323bb2288740adacea3d5d990a0b` |
| `MangaMonitor Dev_1.0.2-rc.3_x64-setup.exe` | `08c1ebf132672781719618f097bcd1c68deee9c3d5b123eedc7e211f531a3437` |
| Packaged `mangamonitor-workbench-preview.exe` | `22eb3392db01efa03a51a6331699948ec135ae9bce127e370a0d306749ca5107` |

A diagnostic build-consistency incident was caught before final measurements:
a detached baseline worktree had reused a Cargo target directory, and a candidate
build reported `Fresh` while retaining the baseline example hash. That artifact
was rejected. Final candidate profiling used a separate target, distinct verified
binary hashes and the same benchmark source; it is the final ABBA evidence above.
No application behavior was inferred from the rejected build.

All eight candidate manifest files match their byte counts and hashes; source
and checkout revisions match. Package, Rust/Tauri config, manifest and runtime
version all report `1.0.2-rc.3`; `production_enabled=false` remains. The NSIS
`UNK`→`NSS` marker is accounted for at offset 19,887,682; CI verifies the exact
permitted binary difference. The candidate is unsigned Dev RC, not a release.
All 20 CI frontend files match the cloud-local candidate distribution byte for
byte. Candidate retention is 14 days; supporting UI/native evidence is seven
days. The final evidence archive preserves the manifests, hashes and CI logs.

Windows CI used WebView2 `153.0.4234.48` already present on the runner. It tested
restarting the exact candidate, independent reader windows, pin/main-hide/restore,
OS-owned HWND close messages with the frontend close handshake, generated
temporary-ZIP recycle with recycle-bin identity proof, and isolated silent
install/reinstall/uninstall with data/shortcut/registration checks. It does not
replace the user's final local acceptance.

Local Windows items still pending:

- Physical window close-button interaction and natural reader use with real
  user media; CI lifecycle evidence explicitly records `mediaRead=false` and
  `physicalCloseButtonTested=false`.
- Explorer's visibly selected file/location, interactive installer pages,
  historical `0.3.4` upgrade and the user's actual shortcut/environment.
- Windows volume-level ENOSPC behavior; syscall fault injection here is Linux only.
- Multi-hour native WebView/media/decode/download/scan resource behavior. The
  cloud browser backgrounds model IPC progress, not native executor CPU/I/O.
- User acceptance of real future special-follow new-work notifications and
  natural per-launch new badges. Synthetic success does not grant acceptance.

Expected candidate behavior for local acceptance: closing an author tab releases
its private saved positions while switching open tabs preserves their anchors;
selection toggles leave source order and tag filtering intact; observations
return the same fresh complete data with lower warm local-commit cost; corrupt
or conflicting files still fail visibly without reset/overwrite. Local approval
to install or touch the real profile remains separate from this task.

## Evidence, reproducibility and continuation

- [Completed original baseline](../evidence/long-regression-2026-10-02/baseline-complete/manifest.json): all raw operation/latency/resource logs and exit/continuity proof.
- [Confirmed failure and short retest evidence](../evidence/long-regression-2026-10-02/tab-lifetime-before/) and [native diagnostics](../evidence/long-regression-2026-10-02/native-diagnostics/): original repro failures are retained.
- [Exact-candidate CI, artifacts and targeted ABBA evidence](../evidence/long-regression-2026-10-02/post-fix-checkpoint-01/manifest.json): completed CI/performance evidence; its live long-test snapshots were explicitly incomplete at capture.
- [Final sustained follow-up archive](../evidence/long-regression-2026-10-02/final-followup/manifest.json): completed summaries, raw data, heap snapshots, comparison CSV, timing-diagnostic qualification/ABBA, analysis scripts and file hashes.

Baseline application is `9afbb4b0470b3939b9f8f8e0848dcc1c3a8b3827`; baseline
long-test harness is `159f4f725200d639d080fd719d9d7fae2eb92936` (application
unchanged). Performance baseline harness is `87086719600aa5be632785adad49672b16692f41`,
using frozen original build bytes. Final mixed-run harness checkout is
`2cb4789251a28a88fa5ce7c045afe546b7181f70`; its app is byte-identical to candidate
`1cba4b81eaf439de9d00c47a42aeb11027b1e1fa`. The performance script, fixture,
workflow fixture, package and lockfile are identical across the compared runs;
`performance-input-consistency.json.gz` records hashes. Closed-tab long retest
uses frontend `25e3c106bc8df58563f4ce3be97298267ed72d9b`, unchanged in the final
candidate. Cloud Chromium is `151.0.7922.173`, Node `24.19.0`, Rust `1.98.1`;
CI uses its pinned Playwright browser. The cloud browser CDN limitation and
Windows/WebView differences are not hidden by the functional pass.

The original baseline checkpoint uses temporary-file rename. Supplemental
JSON snapshots use direct writes; after an abrupt interruption prefer the last
complete NDJSON line if a JSON snapshot is truncated. Do not infer completion
from a checkpoint without its final summary.

Original controller handles were lost twice, but retained PIDs/start times,
advancing logs and read-only zero-exit observations prove the original four-hour
processes continued. No process restart was silently joined. This batch finished
its planned stages; after a future interruption, preserve the original directory
and start a new one, record the exact app/assets/harness/seed and count a new
continuous segment. Never rewrite a partial summary as complete or add parallel
process-hours to a single-process claim. The command templates above apply to
the stated harness/source checkout; build the matching app assets explicitly.
