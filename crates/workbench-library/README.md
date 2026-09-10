# Existing PC library reader

This crate reads only a root supplied by the native folder picker. The renderer
uses opaque SHA-256 root and entry IDs and a scan generation. It cannot choose a
path, URL, extraction destination, downloader, inventory mutation or output file.

The fixed private `library.json` document stores root selection and metadata, using
`WorkbenchStore` revision/CAS and atomic replacement. Restart only reads this
document. A scan interrupted by restart or a failed checkpoint requires an
explicit refresh. No document lock is held during filesystem enumeration or
archive/image parsing. A second instance's revision prevents stale saves.

Each top-level directory is a work; up to eight directory levels are read in
batches of at most 128 nodes, with a 200 ms yield check between nodes. Limits are
20,000 works, 5,000,000 visited nodes, 20,000 nodes and 1,000 directories per work,
and 32 MiB for the private document. Reaching a bound leaves an explicit error,
never a complete scan. No symlink/junction/reparse point is followed: Unix uses
descriptor-relative `openat` and directory iteration; Windows holds ancestor
directory handles against rename and uses reparse-point-aware opens. User media
is opened read-only and is never moved, removed, extracted or rewritten.

Top-level images provide covers; when chapter images exist, only chapter images
count as content pages. `.下载中-` chapter directories are skipped and the item
reports `LIBRARY_DOWNLOAD_INCOMPLETE`. A lone `cover.*` reports zero content pages
and `LIBRARY_COVER_ONLY`. Counts are observations of named image files, not proof
of successful download, image validity, archive CRC integrity or phone presence.
The phone reference inventory is a separate storage document and service.

ZIP/CBZ supports stored and deflated entries. Before ZIP construction, EOCD and
the complete bounded central directory are checked: 10,000 entries, 8 MiB of
directory data, bounded names/offsets, and no duplicate entries. ZIP64, multi-disk,
SFX, encryption and other compression methods are unsupported. RAR remains a
visible unsupported item. Malformed archives are isolated from other works.

Supported metadata consists of bounded UTF-8 ComicInfo.xml (no DTD or external
entities) and the distinct JM/Pica downloader `元数据.json` shapes. Their paths and
URLs never authorize filesystem or network access. Only explicit source IDs from
those documents or exact `[JM:123]` / `[Pica:24-lowercase-hex]` filename tokens
create links; conflicting evidence does not. Manual links and explicit unlinks
persist through refresh when the record's file identity remains unchanged.
Directory identity means the directory object and its modification timestamp;
this does not certify unchanged bytes of all chapter images.

Only a requested visible cover reads image bytes. Its root, generation and file
identity are rechecked. ZIP compressed input is limited to 16 MiB, actual decoded
entry bytes to 32 MiB, image dimensions to 8,192 per side / 24 million pixels;
output is a JPEG of at most 512 pixels per side and 256 KiB. No thumbnail is stored
on disk. The caller owns a bounded cache for the current application run.

Tests use synthetic temporary directories and archives. Formal tests, Clippy and
builds run in CI; no actual user library or source account is used by tests.
