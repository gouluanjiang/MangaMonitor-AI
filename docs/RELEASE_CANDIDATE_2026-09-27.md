# V1 release candidate delivered — user installation acceptance pending

Update 2026-09-29: the user reported acceptance complete and authorized formal 1.0.0 release closure, followed by agent-operated all-author change-summary acceptance. The pending installation statements in this historical delivery record are superseded by that user confirmation; no new agent-observed upgrade trace is implied. See [the current release record](RELEASE_1.0.0_2026-09-29.md).

Implementation, independent review, final CI, candidate verification and local
delivery are complete for `1.0.0-rc.1`. The user accepted the preceding reader,
UI and four project-review corrections at `2493fb5`; the remaining step for this
candidate is user installation acceptance against the existing environment.

Final head: `b487657be1f28565e07ab05256059d870e897c65`; test merge:
`6b9b9a6bd39a4231ee6ede5b27c1eb576d2b553e`.
[Frontend CI](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/36329598153)
passed 239 logic tests and 213 Chromium cases.
[Baseline CI](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/36329598149)
passed both jobs.
[Windows CI](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/36329598229)
passed 23 license/NSIS cases, collected 357 dependency components, and passed
49 native tests, Clippy, the five-step isolated installation lifecycle and
installed-application WebView checks. Formal suites and builds ran only in CI.

The independently verified candidate was extracted to
`Documents/Codex/MangaMonitor-RC-1.0.0-rc.1-b487657`. ZIP CRC, each manifest file's
hash, lockfile/license inventory, version/revision, x64 application PE and the
reverse restoration of the documented NSIS marker to the build hash all passed.
The NSIS bootstrapper is correctly x86; its installed application is x64.

| Verified item | SHA-256 |
| --- | --- |
| Candidate artifact ZIP | `3e2e1d2da718d2e3092851a33020a91791ad76e71fbd158ddce18f43958c2663` |
| Installer | `2010b77728b9b74094114dac19f31c2ad3c325d0cd4ee395bf1c83f92d2fd8f5` |
| Installed executable | `9e7ed3a5e4a450cb3e9f8eb2a729aa6f638b9d0b64d5aff7932963886ac1379d` |

The two native screenshots show synthetic missing-book placeholders and support
the window-lifecycle evidence, not real manga reading. No user app was started or
closed, no installer was run on the user's computer, and no shortcut or library
was changed. The existing Dev delivery remains `2493fb5`; leave that environment
for the user's installation check. The candidate is unsigned. Interactive
installation and an old `0.3.4` to RC upgrade have not been demonstrated.

Private reports, acceptance material and verification receipts are under
`Documents/Codex/MangaMonitor-release-candidate-20260927`. No public release,
tag, PR merge or production enablement occurred; A6 all-author acceptance stays
deferred. These final evidence-only document updates remain local until the next
necessary push and do not change the verified candidate revision.

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

## Preparation and verification history

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

The next run passed actual license collection (357 components), 19 license
regressions, 49 native tests, Clippy and NSIS construction. Its first-install
check then found the pinned Tauri bundler's expected binary patch: it changes
the first `__TAURI_BUNDLE_TYPE_VAR_UNK` marker to the equal-length
`__TAURI_BUNDLE_TYPE_VAR_NSS`, then restores the unbundled executable afterward.
The installer check now constructs only that documented patch in memory and
requires all installed bytes to match; it does not alter either executable or
accept unrelated differences. The delivered executable remains the installed
copy that the native WebView suite actually exercises. This failed validation
did not publish a candidate artifact.

The final revision and completed candidate verification are recorded at the top
of this document. Earlier failed validations remain diagnostic history, not
delivery artifacts. User installation acceptance and public release remain
separate; prior functional acceptance is retained and the deferred all-author
summary acceptance is not restarted for release paperwork.
