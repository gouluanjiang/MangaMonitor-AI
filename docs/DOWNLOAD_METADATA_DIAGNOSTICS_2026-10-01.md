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
CI results will be recorded after the source revision is finalized. No real
download, full author scan, installation, merge or release is part of this work.
