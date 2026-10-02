# Windows local validation follow-up

Base application: `1cba4b81eaf439de9d00c47a42aeb11027b1e1fa`.
Base handoff: `82e88a70df8477187b732e1331aac55fdc8af580`.

The initial 37-case local run reported 10 passes, 1 failure, 25 blocked
cases and 1 untested natural-update case. Those results belong to that
candidate, not a later build. The user subsequently authorized the metadata
fix, normal-user use of the existing application profile and credentials,
and adding special follows for bounded validation. Formal manga files are
excluded from destructive and fault-injection tests. No release or merge is
authorized by this validation work.

## LOCAL-01: recent cards retain stale detail metadata

Opening a detail obtains a newer title/date, but returning to Recent Updates
previously changed only labels. Its independently retained reader did not
receive the successful detail. The same failure was reproduced against the
earlier `19b50a4` frontend, so it is not a new cloud-change regression.

Successful explicit detail reads now notify the recent reader for the exact
source and session. Only identities already in that reader are enriched;
this does not fetch details, append works, change pagination or resort cards.
The compact enrichment shares objects with the display projection and is
removed when its identity leaves the reader. Repeated identical details do
not publish again. Other accounts/sources and disposed readers ignore it.

History and sparse pagination cannot undo an opened detail. A subsequent
successful explicit head refresh may replace its returned identities, while
inheriting known tags/dates omitted by lightweight records. A refresh already
in flight when the detail arrived cannot roll it back. Failure preserves the
last valid metadata. Single and combined source views share the same reader.

Targeted regressions cover this propagation, source/session isolation, late
responses, disposal/unsubscribe, no extra requests, failed refresh, combined
ordering and browser return-position stability. Formal tests/builds run in
the existing CI pipelines. Local retests must record the delivered revision,
manifest and executable hash; pending results must not be reported as passed.

## Windows validation evidence

The private local progress/report records the exact candidate and test times.
Only synthetic fixtures and sanitized diagnostics belong in the repository.
Windows-specific fault helpers must be test-only binaries built by CI;
they must not add a production bypass for source URLs, authentication,
recycling, file identity or download approval. No dependency installation,
ACL modification, permanent-delete fallback or disk-filling test is allowed.

Adding a special follow and establishing its baseline tests that workflow.
An actual future source update is still required to verify natural-update
notifications; rewriting a baseline does not satisfy that requirement.
