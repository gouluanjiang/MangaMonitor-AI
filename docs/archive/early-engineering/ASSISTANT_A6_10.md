# Assistant A6.10 — image download authorization bound to live preflight

Status: implementation/validation on `assistant-a6-image-download-authorization`.

Baseline: `main@1b9b34a037ff91176dce6d4233fa3889e32667ae` (accepted A6.9).

## Goal

A6.10 defines the first gate that may positively authorize image-byte download and staging writes. It still performs no download and no filesystem write itself.

The gate is deliberately bound to the exact in-process A6.9 live result rather than a standalone serialized preflight file:

```text
current state + current gate
    + exact command/plan/request
    + typed A6.9 live preflight result
        -> revalidate A6.9/A6.7 proof
        -> revalidate current A6.8 authorization again
        -> require current state/gate hashes == A6.9 post-check hashes
        -> image download authorized
        -> writes restricted to commands/<command_id>
```

## Live-preflight provenance barrier

A6.10 requires a `LiveSourcePreflightResult` produced by the A6.9 live path. The authorization function rejects the input unless:

- the A6.9 schema is exact;
- metadata enumeration completed;
- A6.9 marked the authorization generation stable;
- A6.9 pre/post state hashes are equal;
- A6.9 pre/post gate hashes are equal;
- command/task/work/revision/target/source/source-work binding matches the A6.6 request;
- the A6.9 result itself did not claim image-download, staging-write, or downstream mutation authority.

The A6.7 proof is then recomputed from the exact A6.9 evidence and must equal the embedded proof byte-for-byte at the structured value level.

## Current-generation recheck

After validating the live result, A6.10 runs the accepted A6.8 authorization function again against the current state/gate.

The current state/task binding hash and current gate-ledger hash must still equal the hashes recorded at the end of A6.9. Therefore any task/gate change after preflight forces the caller to repeat the live preflight before image download can be authorized.

## Positive authority

A successful A6.10 result may set only:

- `image_download_authorized=true`;
- `staging_write_authorized=true`.

The write scope is fixed to `COMMAND_OWNED_STAGING_ONLY`, and `staging_subdir` is the exact A6.2 `commands/<command_id>` namespace.

The result also carries the exact A6.7 `preflight_hash`, expected chapter count, and expected content-unit count so a later downloader can bind all work to the verified expected scope.

## Non-reusable authorization

`ImageDownloadAuthorization` is Serialize-only and declares `reusable_permit=false`. It is diagnostic/audit output, not a transferable capability.

The later image downloader must obtain the authorization in-process immediately before starting its command-owned staging operation. No A6.10 CLI accepts a saved A6.9 result as an input permit.

## Downstream authority remains closed

A6.10 always keeps these false:

- inventory mutation;
- task completion;
- promotion;
- replacement;
- physical deletion.

A6.10 creates no directory, writes no file, performs no source request, and mutates no monitor state.

## Acceptance criteria

A6.10 is accepted only when:

- exact typed A6.9 result binding is required;
- A6.7 evidence/proof is recomputed and must match;
- current A6.8 state/gate authorization is rechecked;
- any state/gate generation drift since A6.9 fails closed;
- revoked approval fails closed;
- changed task generation fails closed;
- forged preflight proof fails closed;
- unsafe A6.9 capability flags fail closed;
- positive write scope is exactly `commands/<command_id>` only;
- authorization is non-reusable;
- all downstream mutation/promotion/delete authority remains false;
- workspace tests and Clippy pass;
- existing assistant offline/read-only regression passes;
- `production_enabled=false` remains closed.

## Next gate

Only after A6.10 is accepted may the source adapters expose image-byte download descriptors/operations to a command-owned staging writer. That downloader must use the exact A6.10 `preflight_hash`, produce one canonical artifact per expected image, join all scheduled work, and generate the accepted A6.5 completion transcript for A6.3/A6.4 verification.
