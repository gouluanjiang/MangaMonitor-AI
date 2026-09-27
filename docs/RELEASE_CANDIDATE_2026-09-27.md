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

Implementation, independent review, CI packaging/lifecycle checks and candidate
artifact verification are in progress. No candidate-installation acceptance or
public-release completion is claimed. Record exact final evidence here before
delivery. Prior functional acceptance remains valid; the deferred all-author
summary acceptance is not automatically restarted for release paperwork.
