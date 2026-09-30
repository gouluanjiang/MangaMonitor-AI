# Author catalog coverage repair

## Problem and scope

A followed creator's work can be visible in a source's recent feed while absent
from author updates. The previous implementation used a general keyword query
for JM authors, treated the query that originally found a record as an attribution
boundary, and did not retain other browsing discoveries in the author catalog.
Refreshing the local display did not perform a new source check.

This maintenance batch repairs those paths. It does not promise access to works
that every source endpoint omits, change the library identity model, or infer
authorship from a work title. The installed 1.0.1 release and its acceptance remain
separate from this candidate.

## Contract

- JM author queries use the source's author field (`main_tag=2`). General keyword
  and tag searches retain their respective routes. Pica uses its supported search
  route followed by exact credit verification.
- Attribution considers all currently followed names, verified aliases, explicit
  compound credits and reviewed source/work-specific corrections. The original
  query that found a work remains provenance, not an exclusion rule. A short name
  can have valid exact credits even when automatic broad queries are disallowed.
- A work has one source/ID record and may belong to several followed authors.
  Removing a follow removes that membership without discarding raw evidence.
- Source-account observations from favorites, recent updates, search, ranking and
  detail are retained as metadata. A later abbreviated list cannot erase known
  detail credits. Timestamp/evidence precedence handles out-of-order responses.
- Observation persistence and author-catalog incorporation are distinct. If an
  author scan currently owns the catalog, observations remain queued for replay.
  Failed storage never signals a successful catalog change or advances a recent
  coverage checkpoint.
- Author pagination baselines are advanced only by author queries. Observed works
  do not manufacture a completed range, a new author scan, or its last-check time.
- All author entrances can supplement source query results with already saved,
  credit-confirmed works. Supplements do not affect raw page counts or source
  pagination boundaries. Missing account context is reported as incomplete local
  history availability, not as proof that a work is absent.
- Recent-feed duplicates are hidden only after the exact source/ID is in the
  confirmed author catalog. Nonfollowed and still-unconfirmed records remain in
  the recent history. Explicit content filters and ownership filters remain
  independent; unknown tags do not cause a full detail-request fanout.
- JM's explicit English Manga category remains outside the agreed JM author
  scope. Raw rows still participate in pagination accounting. Pica is not given
  an equivalent blanket language exclusion.

## Recent-feed coverage

Recent coverage is separate from author coverage. The first check establishes an
explicit bounded continuous window (eight pages by default; the local audit may
request a larger initial window). It is labeled an initial window, never the
source's complete historical archive. Subsequent checks continue until the prior
ordered head sequence is joined or the source reports its end. Unverified repetition,
isolated malformed rows, storage failure, source errors and exhausted traversal
limits leave a partial result and preserve the last successful boundary.

JM's recent all-category endpoint reports a 10,000 listing ceiling. This is not
proof of an exact distinct-work count or of the source's historical end. A
bounded diagnostic also observed pages beyond the available window repeating
its last page unchanged; that repetition does not establish longer coverage.
The audit retains the failed larger attempt and explicitly records a smaller,
verified readable window rather than claiming the whole site was traversed.
The same endpoint can replay an earlier, ordered fragment across several
pages. A recent-only recovery path accepts that fragment only when every source
row and projected work exactly match a contiguous part already read. New rows
may follow a repeated prefix only after it reaches the known tail. Reordered,
changed, unverifiable or noncontiguous repeats remain incomplete. Full replay
pages have a bounded consecutive limit and cannot alone finish an initial
coverage window. If the initial window ends during a proven replay, JM may use
at most three additional recovery pages to reach the known tail and obtain new
records. The result reports those requests separately and the coverage stores
the actual page count. An unresolved replay remains incomplete; other sources
and author queries receive no extra requests. Every requested row still consumes
the unchanged overall traversal budget.

This uses transient per-row raw hashes supplied only by the native JM recent
parser; persisted or IPC-deserialized pages cannot manufacture that proof.
Author/search traversal and Pica duplicate rules remain unchanged. A source
pagination change still reports an incomplete range, preserving observations
without advancing a successful coverage checkpoint.

The source can still change its index, reorder results or omit records. The UI
must show the actual check range and unresolved errors rather than claiming that
all published works everywhere have been found.

## Verification and delivery

Application revision `81d06ed` passed the baseline, UI and Windows desktop CI,
including isolated installation and restart checks. Helper-only revision
`df95488` separately passed its synthetic contracts and optimized build; it does
not change the application runtime. The original gap retests, full followed-author
checks on both sources, bounded recent traversal, saved-data cross-check and
targeted residual-detail investigation have completed. The final offline audit
has no failed or unexecuted checks; local/source credit disagreements remain
explicit review leads, not confirmed software omissions. User acceptance remains
open. Real metadata verification runs locally through normal
account restoration, with private plans and reports outside Git. The verification
sequence is individual known-gap retests, all saved evidence cross-check, all
followed creators on both sources, continuous recent-feed supplementation, and
investigation of new discrepancies. No manga download is part of that sequence.

Application CI: baseline `36713650365`, UI `36713650445`, Windows desktop
`36713650448`. Helper-only synthetic tests and release build: `36720418443`.
Public raw-field samples agreed with the native detail parser, including an empty
author array, a translation-group credit and an orthographic difference. These
samples do not establish the identity of every unresolved local credit. A saved
work may already appear under another confirmed contributor while a local credit
is still unresolved; neither count should be presented as a missing-work total.

Preserve download history, library identities, current follows and the original
metadata backup. A passed build is not a passed live check or user acceptance.

## Large-directory recovery and audit control

An interrupted audit may have saved its complete author ranges before finishing
journal maintenance. Observation writes now reuse their validated catalog read
to atomically checkpoint a journal at 256 patches or 16 MiB. This preserves the
logical revision, raw records and pagination baselines; the storage-level
`read_discovery` remains read-only, and a failed checkpoint leaves the old complete
journal replayable. Application catalog views may still merge saved observations.

The local helper retries only transient BUSY reads, never scan starts or source
queries. An optional private `--cancel-file` marker requests cooperative
cancellation; final protected-file checks wait for source and storage workers to
settle. Process termination is not a substitute for that barrier. CI builds the
helper with optimizations; no live plans, account data or real works run in CI.

The `evidence` mode retains individual author-entry checks for the original
acceptance cases. The `details` mode is for larger residual-credit investigations:
it queries each planned exact source ID once, then evaluates membership against
the final native catalog and existing native author policies. Its report labels
that evidence separately and does not claim individual author-entry verification.
Recent-history exports are read-only supplements to a separately saved traversal
receipt; they cannot create or extend a successful coverage boundary.
