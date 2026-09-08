# Assistant A6.12 — isolated command-staging execution

Status: implementation/validation on `assistant-a6-isolated-staging-execution`.

Baseline: `main@6a213242c44bd777c9d52eac7e038fa846fa6924` (accepted A6.11).

## Goal

A6.12 introduces the first layer allowed to write media bytes into the exact A6 command-owned staging namespace. It does not yet own a JM/Pica source network client. Instead, it consumes already-processed bytes from an in-process fetch/process function after revalidating the complete A6.10/A6.11 chain.

The boundary is:

```text
current A6.10 authorization
  + exact A6.7 evidence/proof
  + exact A6.11 descriptor set
  + current-generation reauthorization callback
      -> fresh commands/<command_id> tree
          -> exact processed media bytes
              -> create-new artifact writes
                  -> A6.5 source completion normalization
                      -> A6.3 manifest validation
                          -> A6.4 filesystem verification
```

No successful A6.12 result exists unless every layer above succeeds.

## Current-generation checks

A6.12 requires the same non-transferable A6.10 authorization generation before any filesystem mutation. The supplied reauthorization callback is then checked:

- before creating the command directory;
- before every media fetch/process operation;
- after each fetch/process operation and before writing the file;
- after all media writes and before completion normalization;
- after filesystem verification immediately before returning success.

Any state/gate generation change fails closed. Already-written command-staging bytes may remain, but A6.12 deliberately never converts them into success evidence after the generation changes.

## Fresh command tree and no overwrite

The staging root and its `commands` directory must already exist as real directories and may not be links/reparse points.

A6.12 creates the exact `commands/<command_id>` directory with create-new semantics. If that directory already exists, execution fails rather than reusing or overwriting it. Chapter directories are created only below that fresh command tree and artifact files are opened with create-new semantics.

A6.12 never deletes partial output. A failed or revoked execution therefore cannot silently clean up and retry under the same generation; cleanup remains a separate future authority.

## Processed-media binding

The in-process fetch/process implementation must return a `ProcessedMedia` object that exactly echoes the accepted A6.11 descriptor:

- source media ID;
- request URL;
- source format;
- applied transform;
- applied transform parameter.

The bytes must be non-empty and carry the expected image-format magic. A mismatched URL/media ID/format/transform or obviously wrong byte format fails before the artifact write.

This makes A6.12 source-independent while keeping the exact A6.11 media and transform contract intact. A later source-specific phase must implement the real JM/Pica fetch/processing function; it may not choose different URLs, paths, or transforms.

## Completion and filesystem proof

After all media writes, A6.12 deterministically builds the A6.5 `SourceCompletionTranscript` from the accepted A6.7/A6.11 scope:

- exact source/work/command/task generation;
- exact chapter identities/order;
- exact Pica pagination evidence where required;
- every chapter scheduled, joined, terminal `COMPLETED`;
- completed image count equals expected image count;
- zero failed images;
- exact chapter artifact paths;
- SHA-256 and non-zero size for every staged artifact.

The transcript is normalized through A6.5/A6.3 and the actual staging tree is then verified through A6.4. Success is impossible if the manifest and filesystem differ.

## Authority boundary

A6.12 still does not:

- own or expose a real JM/Pica network downloader;
- mutate inventory;
- complete pending tasks;
- promote staging content into the archive;
- replace old content;
- delete partial or old content;
- enable production.

Every inventory/task/promotion/replacement/delete capability remains false.

## Next safe phase

After A6.12 passes, A6.13 can implement the pinned source-specific fetch/process functions that consume only exact A6.11 descriptors. JM must reproduce the pinned image request behavior and WEBP unscramble transform; Pica must fetch the exact approved `/static/` media URL. Those source functions should plug into A6.12 rather than gaining direct filesystem or downstream mutation authority.
