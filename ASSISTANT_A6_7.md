# Assistant A6.7 — source enumeration preflight

Status: **implementation plan** on `assistant-a6-source-preflight`.

Baseline: `main@182193c4c1bdd09b4f9d3576267763b4cf38570e` (accepted A6.6).

## Goal

A6.7 introduces a source-specific **read/preflight** boundary between the disabled A6.6 source-bridge request and a later real downloader. It establishes the complete expected source scope before image bytes may be downloaded.

The intended flow is:

```text
accepted A6.6 request
    -> pinned source preflight
        -> complete chapter enumeration
            -> complete per-chapter image enumeration/accounting
                -> deterministic expected-scope proof
                    -> image download still disabled
```

A6.7 is not the real downloader. It may eventually permit only the minimum network reads required to enumerate the exact source scope, while staging writes, image-byte download, inventory mutation, task completion, promotion, replacement, and deletion remain closed.

## Pinned upstream facts

### JM

Pinned upstream: `lanyeeee/jmcomic-downloader@f0cdd724af6892002f2fb7be883b88832cebe7e9`.

The accepted worker derives chapter image work from:

- `/chapter?id=<chapter_id>` for the chapter image filename list;
- `/chapter_view_template?id=<chapter_id>` for the scramble ID used by image reconstruction;
- image URLs under the pinned JM image domain.

The upstream worker joins every image task and compares downloaded image count to total image count before reaching terminal `Completed`. A6.7 will use only the enumeration side of this behavior; it will not treat image URLs as downloaded artifacts.

### Pica

Pinned upstream: `lanyeeee/picacomic-downloader@77c8b62ede42b3afc074506d092313816af8092d`.

The accepted source paths are:

- `comics/<comic_id>/eps?page=<N>` for whole-work chapter pagination;
- `comics/<comic_id>/order/<chapter_order>/pages?page=<N>` for chapter image pagination.

Every page `1..=total_pages` must succeed. The preflight must never reproduce the upstream whole-comic helper behavior that can omit a failed later chapter page.

## Expected-scope proof

A6.7 will define a deterministic proof bound to the exact A6.6 request containing at minimum:

- command/task/work/revision/target binding;
- source and source work ID;
- pinned upstream commit;
- `FULL_SOURCE_WORK` scope;
- exact ordered chapter IDs/orders;
- exact chapter count;
- per-chapter exact expected image count;
- complete pagination evidence where the source requires it;
- total expected image/content-unit count;
- source enumeration complete flag;
- no credential material;
- no downloaded image bytes;
- no artifact-completion claim.

The proof is only an **expected scope certificate**. It cannot become A6.5 completion evidence by itself. Later real execution must demonstrate that the exact preflight scope was actually downloaded, joined, hashed, and verified.

## Safety boundary

A6.7 must keep all of the following false:

- image-byte download authority;
- staging write authority;
- inventory mutation authority;
- task completion authority;
- promotion authority;
- replacement authority;
- physical deletion authority.

Pica credentials, when a later local preflight runner needs them, must be supplied out-of-band and must never appear in serialized request/proof JSON, logs, fixtures, or Git history.

## Initial implementation steps

1. Add a source-independent expected-scope proof contract in `cloud-monitor` bound to A6.6.
2. Add adversarial offline tests for incomplete/duplicate/non-canonical chapter and image enumeration.
3. Add pinned JM/Pica adapter enumeration primitives without filesystem writes.
4. Add local preflight CLI/runtime gating separately; network preflight must remain explicit and must not be enabled merely by deserializing an A6.6 request.
5. Only after the expected-scope proof is stable may a later phase download image bytes into command-owned staging and generate observed A6.5 completion evidence.

## Acceptance criteria

A6.7 is accepted only when:

- exact A6.6 request binding is preserved;
- upstream commit and completion-contract version remain pinned;
- chapter enumeration is complete, canonical, non-empty, and duplicate-free;
- per-chapter image enumeration is complete and positive;
- Pica chapter and image pagination are exact `1..=total_pages` with zero failed pages;
- JM chapter/image IDs and counts satisfy the pinned source shape;
- total expected content units equal the sum of chapter image counts;
- forged request/source/upstream/scope bindings fail closed;
- no credential value is serializable into proof output;
- no image byte is downloaded in the offline contract tests;
- no staging/archive/monitor-state mutation occurs;
- all prior workspace tests and Clippy pass;
- `production_enabled=false` remains closed.
