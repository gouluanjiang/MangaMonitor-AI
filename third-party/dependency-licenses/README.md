# Exact dependency license supplements

The first Windows candidate collection stopped because these 15 published
crates omit standalone license text. Archive listings were checked rather than
relaxing the collector's filename rules. Every `crateSha256` in `manifest.json`
matches the exact package checksum in the native `Cargo.lock`.

The collector applies a supplement only when the installed crate has no license
text, the name/version and declared license match exactly, and its
`.cargo-checksum.json` confirms the pinned archive. Where the published archive
records a VCS commit, `.cargo_vcs_info.json` must match too. Each supplemental
text is checked against its raw SHA-256. `.gitattributes` disables newline
conversion for this directory. Other missing/invalid licenses remain errors;
upgrading a dependency does not inherit its old exception.

The generated text and inventory retain each public text source, source archive
download URL, archive checksum, provenance and known release commit. Original
source archives remain available at those exact versioned URLs; MangaMonitor
does not modify these dependencies. The supplements are included in the two
generated license resources, not copied from an entire registry.

| Packages | Preserved material |
| --- | --- |
| tauri-plugin 2.6.3 | MIT and Apache texts from its published VCS commit. |
| unic-char-property, unic-char-range, unic-ucd-version, unic-common 0.9.0 | MIT/Apache texts, COPYRIGHT.md and AUTHORS at the published commit, plus the original source header. COPYRIGHT.md preserves Rust, rust-url, Servo and UNIC attribution. |
| unic-ucd-ident 0.9.0 | The same set from its own published commit; its source header records 2017-2019 UNIC attribution. |
| defmt-parser 1.0.0 | MIT and Apache texts from its published VCS commit, including Ferrous Systems attribution. |
| alloc-stdlib 0.2.4 | The upstream BSD-3-Clause text, including Dropbox attribution. |
| selectors 0.36.1 | Its original source header explicitly refers to Mozilla MPL 2.0. Neither the archive nor that repository root contains the standalone text; the supplement is the official Mozilla version-2.0 text and is marked `standard-license`, with the published header retained separately. |
| zune-core 0.4.12, zune-jpeg 0.4.21 | The complete upstream Zlib text, selecting that option from the declared MIT OR Apache-2.0 OR Zlib expression. The upstream LICENSE.md is also retained as an attribution notice, not treated as complete MIT/Apache terms. Each crate's own VCS commit is pinned; the retrieved texts are byte-identical. |
| webview2-com 0.38.2, webview2-com-sys 0.38.2, webview2-com-macros 0.8.1 | The upstream MIT text and attribution. The macros crate has a distinct recorded VCS commit with the same license bytes. |
| unicode-casefold 0.2.0 | The published manifest declares MIT/Apache-2.0 but the archive has no license/NOTICE or VCS metadata; the upstream repository also lacks license text. This one reviewed legacy entry selects Apache-2.0 using the official Apache standard text. The published Cargo.toml is preserved unchanged for its author and declaration. Release commit remains explicitly unknown; an older matching-source commit whose manifest says 0.1.0 is not claimed as the 0.2.0 release. |

Standard-license entries above are explicit component-specific decisions, not
a general substitution of SPDX identifiers for missing upstream evidence.
