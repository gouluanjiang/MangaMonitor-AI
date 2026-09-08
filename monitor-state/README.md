# Monitor state

This directory is the commit-aware state target for the cloud monitor. During
Phase 3B it contains only the five-author validation seed and production remains
disabled in `monitor-config.json`.

The scan binary never edits this directory. It writes a complete staged state to
a report directory. The commit helper copies only the declared state files,
checks that `origin/main` still equals the commit used to start the scan, and
then creates a normal fast-forward commit. A conflict leaves the staged result
as an Actions artifact for recovery.
