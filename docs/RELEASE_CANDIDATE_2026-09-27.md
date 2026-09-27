# V1 release-candidate preparation

The user accepted all four project-review corrections at `2493fb5` and authorized
the next planned step. Reader and UI work is accepted; the current next step is
formal distribution preparation, not a new product-feature batch.

## Candidate contract

- Prepare version `1.0.0-rc.1`, using the current Windows x64 per-user NSIS path.
- Retain product name `MangaMonitor Dev` for the candidate. The pinned Tauri CLI
  2.11.4 NSIS template uses PRODUCTNAME in its uninstall key and default install
  location. A cosmetic rename must not silently break old installation lookup.
  The main window can identify itself as a release candidate.
- Retain `com.mangamonitor.workbench.preview`, `workbench-preview-v1`, existing
  credential namespace and internal binary name. No migration or private-store
  rewrite is introduced by changing the version.
- Correct current entry-point documentation, retaining original README material
  as historical files. Bundle current instructions and license material.
- Collect dependency notices from the locked, installed npm production graph and
  Windows native normal/build graph. Publish package identifiers, license texts,
  hashes and relative filenames, not build-machine paths or credential metadata.
- Use the existing CI once for each revision/target; no local duplicate suite or
  native build. Candidate packaging must not publish an installer as verified
  before its checks pass.
- Exercise a synthetic per-user install, installed EXE, same-candidate reinstall,
  uninstall preserving app data and reinstall in disposable Windows CI. This is
  not an old-0.3.4-to-new upgrade or physical user-machine installation claim.
- Produce a version/revision/hash manifest and deliver a reviewable candidate.
  The build remains unsigned; WebView2 bootstrapping may require network.

No updater, source protocol, media pipeline, automatic identity matching or
library mutation behavior changes. No real source queries, downloads, user-data
edits, app shutdown, actual local installer execution or full-author replay are
part of this preparation. `production_enabled=false` remains unchanged. Public
tag/release and PR merge are separate final actions, not implied by candidate
generation. This document does not authorize them.

## Verification status

Implementation and independent review are complete. The first Windows run at
`c2bed62fad0339b596e0444af1ac42a7fd995f0d` correctly stopped before packaging:
15 published Cargo archives omit standalone license text. Their exact archive
checksums were verified against the native lockfile. The supplement manifest
pins the package name/version, declaration, archive hash, known VCS commit and
each raw text hash. Missing text in other packages still fails. Selected Zlib
and explicit standard-license exceptions are documented alongside the source
material; unknown release commits are not invented. Independent review checked
all 15 archive identities and 41 mappings covering 22 distinct text files.

The material correction is `4c53b0a8468ded938e5a05aa0ec539c529e8e3b3`. It changes only
license collection/material and the affected CI triggers. The entire
`apps/local-workbench` tree is unchanged from the first candidate commit.
GitHub nevertheless automatically triggers frontend CI against the cumulative
PR diff; no additional local suite or manually duplicated build was run.

A second Windows run exposed an incorrect collector assumption: the ordinary
Cargo registry source does not contain vendor-only `.cargo-checksum.json`.
The follow-up hashes the actual exact-version `.crate` under the corresponding
registry cache instead; fixtures now model the real `registry/src` and
`registry/cache` siblings. Missing or changed archives still stop packaging.
Neither failed run produced a candidate installer for delivery.

CI packaging/lifecycle checks and candidate artifact verification are in
progress. No candidate-installation acceptance or public-release completion is
claimed. Prior functional acceptance remains valid; the deferred all-author
summary acceptance is not automatically restarted for release paperwork.
