# Source language badges

The user accepted the download queue cleanup and authorized the next scheduled item: show the website's language labels directly on manga cards. They explicitly accepted the literal Pica 生肉 label as untranslated, not necessarily Japanese. This batch changes metadata retention and presentation; it adds no source detail sweep, filter, OCR, title inference, version matching or automatic download.

## Display contract

| Explicit evidence | Badge | Meaning |
| --- | --- | --- |
| Chinese or translation language label | 已汉化 | The website marks Chinese/translation; the tooltip explains Chinese originals are also possible. |
| Japanese language or literal 生肉 label | 生肉 | Untranslated; 生肉 alone does not prove Japanese. |
| No accepted evidence, or both kinds | 未知 | Missing evidence or a conflict, explained in the tooltip. |

Trim labels and compare English case-insensitively. Accept exactly `中文`, `汉化`, `漢化`, `简体中文`, `簡體中文`, `繁体中文`, `繁體中文`, `中国語`, `中國語`, `chinese`; or `日文`, `日语`, `日語`, `日本語`, `japanese`, `生肉`. General 日漫 categories, title text, author names, translation-team names and substring matches are not language evidence. Absence of 生肉 does not imply Chinese. All entry points use the same frontend classifier, with the same allowlist in native normalization.

Favorites, search, followed-work/author results, author updates, rankings and details show the badge. The PC library also shows it, but only from metadata saved with that local version. Remote source cache never overrides a local-version badge. Older local ZIPs without language metadata remain unknown. Badges sit in the lower cover corner and retain cover aspect ratios, density, selection controls and existing click targets.

## Source and cache boundaries

Read-only review of the pinned upstream schemas and existing repository fixtures found: JM list/search/ranking metadata does not consistently provide tags; its detail does. Pica favorites provide categories; Pica search/ranking/detail provide tags and categories. This describes inspected schemas and fixtures, not a claim that every current website response supplies language metadata. Normalize explicit Pica category language into existing tags. Keep all other source fields and per-item parse isolation unchanged.

Reuse metadata already read for the same source, session and exact work ID. Fresh explicit language evidence, including conflict, wins; a lightweight response without explicit evidence may inherit the prior language labels. Compact collection caches retain at most one label per kind. Revalidating an unchanged cached first page can enrich that page's language without fetching its tail or starting detail requests. Old cached records without evidence stay unknown until their ordinary read/refresh supplies it.

No persistent schema or migration is introduced. Raw source tags remain capped at 64; at most two normalized language labels may supplement them, and discovery storage accepts 66. Existing per-work and total byte budgets remain enforced. Optional enrichment must not turn a previously readable work into a rejected result. A known Chinese/raw conflict must not silently become a one-sided language claim. Accounts/session changes and different source IDs never share these labels.

## Verification and delivery

Local work is editing, formatting and independent static review only. Formal checks, browser tests and Windows builds run in CI, without repeating them locally. Added synthetic coverage targets exact/negative labels, conflicts, Pica categories, metadata compaction, byte/tag budgets, same-ID/session inheritance, cached-head revalidation without extra queries, all affected card entry points and local-version isolation. Browser evidence includes favorites, search, author search/updates, rankings and library cards.

Final head `e8b684145744eccf347c4f2073ff974b84adc237`, test merge `8020af4f14f4046ef70d6b5f8b6f5e1b8ba2157c`, passed [UI CI](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/36235097924) (202 logic / 174 Chromium), [baseline CI](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/36235097911), and [Windows desktop CI](https://github.com/gouluanjiang/MangaMonitor-AI/actions/runs/36235097977) (storage/library/accounts/sources/downloads, 39 native IPC, Clippy, EXE and actual isolated WebView startup/restart). Seven affected synthetic UI captures were visually reviewed against the accepted layout. All formal checks/builds ran only in CI; the first pushed revision passed without a retry.

Delivery: `Documents/Codex/MangaMonitor-Dev-20260926-e8b6841/mangamonitor-workbench-preview.exe`, SHA-256 `a1d8ccb7a96bd85f87cf39120ea8f48d09a0210f210c17ababfe0b198ffaf961`. Artifact digest, CRC, x64 PE and embedded revision passed. Both existing Dev shortcuts point to it; previous shortcuts and executables remain. No user app was launched or closed. Exit the old process and reopen Dev for user acceptance. Private report, visuals and receipts are under `Documents/Codex/MangaMonitor-language-badges-20260926`. Final evidence-only notes stay local for the next necessary push. Dev remains 0.3.4, PR #19 draft/unmerged and production disabled; no installer or formal release was created.

No real account query, author scan, manga download, profile/library mutation or user app control ran for this batch. The user explicitly accepted this batch on 2026-09-26; experience acceptance is complete.

Next scheduled work is A6 author-check change summaries, then website recent updates, the built-in reader, UI/interaction consolidation and formal release preparation. Optional language filtering is not adopted.
