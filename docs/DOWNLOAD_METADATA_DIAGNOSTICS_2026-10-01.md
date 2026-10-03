# Download metadata diagnostics

The approved scope is generic validation and diagnostics using synthetic data.
No real work is used as a fixture, no source request or media operation is part
of verification, and this batch does not claim to resolve a particular work.

## Change

Preparation previously collapsed missing detail, mismatched source identity and
all invalid metadata fields into `DOWNLOAD_METADATA_INVALID`. Direct card or
batch submission could fail before any queue task existed; a later successful
queue read left no usable diagnosis in the settings summary.

- Keep the exact existing acceptance limits and source identity rules. A shared
  validator now returns a stable field-specific error, without trimming,
  truncating, rewriting or admitting previously rejected metadata.
- Distinguish missing detail from a source/work-ID mismatch in the existing
  first-result selection. Single and batch preparation share this check.
- Use one exact error-message allowlist for inline feedback and diagnostics.
- Retain the latest 20 metadata preparation failures for this process in the
  settings diagnostic summary, including failed direct selections and batches.
  Store only source, fixed occurrence time and allowlisted code. Queue refresh
  does not erase them; a fresh process starts empty. Cancelled/obsolete responses
  do not append a new entry. Queue polling remains independent of item failure.

## Compatibility and authority

There is no document-format or migration change. Existing task binding bytes,
download/download-history records, source metadata and library files are not
rewritten. This change does not relax admission or enable execution. The current
source/session/root/revision and CI live-execution gates remain in force.

The download thaw gate was reviewed: no source protocol, image fetch, staging,
promotion, replacement, deletion or completion behavior changes. Existing
upstream pins and execution reviews remain applicable to their prior scopes.

## Verification

Synthetic regressions cover old-rule equivalence, Unicode character and byte
boundaries, every rejected field, redacted codes, empty/mismatched detail,
unchanged saved records after rejection, single/direct/batch propagation, mixed
sources, bounded retention, successful polling, cancellation, late response,
changed sessions and a fresh controller. Browser verification navigates from a
mock failure to settings and through a queue refresh with external network
requests blocked and no confirmation/execution call.

Formal verification uses the existing baseline, UI and Windows CI workflows.
The browser test allowlist now explicitly includes the diagnostic panel suite;
previous green runs that omitted this file do not prove its behavior. The new
scenario uses an already-saved synthetic record and the current queue navigation,
with no synthetic scan required. Unexpected mock commands fail the test.

The exact checked revision and final workflow links are recorded in
[draft PR #24](https://github.com/gouluanjiang/MangaMonitor-AI/pull/24).
Synthetic engineering verification is separate from real-source acceptance.
No real download, full author scan, installation, merge or release is part of
this work.

## Follow-up: whitespace controls in new tag metadata

The diagnostic-only candidate a0e868a was installed after the user's explicit
installation request. Its failure message confirmed the tag-control branch,
but did not identify a particular raw tag or code point. The subsequent generic
fix is authorized for user-operated acceptance, with synthetic engineering checks.

The source reader may preserve whitespace controls in a tag while the download
ledger forbids controls. New-plan preparation now replaces only characters that
are both Unicode controls and whitespace (TAB, LF, VT, FF, CR and NEL) with spaces.
It re-runs all existing bounds before collapsing the changed tag's whitespace.
Valid metadata is returned byte-for-byte unchanged. Non-whitespace controls,
blank or oversized tags, too many tags and unrelated invalid fields still fail.
Words are separated, not concatenated; tags are not dropped or truncated.

Normalization happens before new plan identity/approval binding and confirmation,
in the shared single/batch service path. Saved tasks are never normalized during
read, retry or validation. No schema migration, account action, new source request,
content-filter rule, image pipeline or file-operation authority is added.

Synthetic regressions cover both sources, all six whitespace-control types,
non-whitespace C0/C1 controls, raw-length limits, empty labels, unchanged valid
metadata, idempotence, preserved old records, normalized bindings, restart reads,
and rejection without queue/library mutation. Preparation and isolated ledger
confirmation do not execute downloads. Real-source success remains for the user
to verify; a remaining non-whitespace control will still produce the diagnostic.
