# Matcher M3 old-state fixture

`phase3b-old-state/` is a read-only copy of the eight persisted state files from
the existing Phase 3B first-run snapshot (`run 33998989019`). It is committed so
the M3 production-path migration test has the same deterministic 215-review seed
on both Windows and a fresh Linux Actions checkout.

The test and workflow hash the fixture before and after both replays to prove the
seed is not modified. Generated checkpoints, reports, observations, and exports
remain outside this fixture.
