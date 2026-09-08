# Assistant A2 — read-only delivery bundle

## Goal

A2 turns the A1 in-process/CLI views into a portable, immutable assistant bundle that ChatGPT can read from GitHub or an Actions artifact without being given write capability and without requiring AI during collection.

A2 is still **read-only** with respect to MangaMonitor state.

## Why a bundle is needed

A1 can derive bounded views from a state directory, but ChatGPT should not need to load the full catalog/inventory or run Rust itself for routine questions. A2 therefore exports a small deterministic snapshot from a consistent state generation.

The bundle is derived data. It is never source truth and can always be regenerated from the eight durable MangaMonitor state files.

## Bundle contents

A complete bundle contains:

- `manifest.json`
- `scan-summary.json`
- `review-summary.json`
- `pending.json`
- `collection.json`
- zero or more `review-batch-XXXXXX.json` files

Review batches use the existing A1 bound (`MAX_REVIEW_BATCH`, currently 100). The exporter chooses a requested batch size at or below that maximum.

## Manifest

The manifest binds every assistant view to one exact input-state generation.

It records:

- assistant bundle schema version;
- assistant view schema version;
- source scan ID/status;
- review batch size/count;
- unresolved review count;
- sorted output filenames;
- SHA-256 of each of the eight durable input state files;
- one aggregate state hash derived from those file hashes.

This prevents a consumer from accidentally mixing views produced from different state generations.

## Export semantics

`assistant-export`:

1. loads the state through the existing strict persistence loader;
2. hashes all eight durable input files;
3. derives A1 views in memory;
4. writes into a new output directory only;
5. refuses to overwrite an existing output path;
6. performs no HTTP/source request;
7. never mutates the input state;
8. fails closed if the state is malformed or a candidate reference cannot be resolved under A1 rules.

A caller that wants to publish/commit a bundle must first generate it in staging and then use a separate commit-aware publication step. The exporter itself never commits or pushes Git state.

## AI-offline property

Bundle generation is deterministic Rust and does not call ChatGPT or any OpenAI API. Therefore scheduled collection can continue and bundles/backlogs can accumulate even when the user has no ChatGPT subscription or AI quota.

## Privacy boundary

A2 inherits A1's sanitized evidence boundary. The bundle must not contain credentials, cookies, authorization headers, raw image/archive payloads, or arbitrary local filesystem paths.

## A2 acceptance criteria

A2 is complete when:

- all imported tests still pass;
- A1 tests still pass;
- `assistant-export` creates a complete bundle from a tracked consistent state fixture;
- repeated export from the same state produces byte-identical JSON files except for no time-dependent fields (A2 introduces none);
- every input state file remains byte-identical;
- an existing output directory is refused rather than overwritten;
- review batches are complete, non-overlapping and bounded;
- manifest hashes match the actual eight input files;
- the export succeeds while HTTP/HTTPS/ALL proxies point to invalid loopback endpoints;
- clippy passes with warnings denied;
- `production_enabled=false` remains unchanged;
- no original No-AI repository write occurs.

A2 does not add author writes, decisions, download approvals, downloads, replacement, coverage changes or deletion.
