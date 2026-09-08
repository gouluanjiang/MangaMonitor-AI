# Phase 3A sanitized evidence

Source observations came from private GitHub Actions run `33983528241`, commit `a125a28bc6375043fc7c23026a43f3c03b7e3100`. The five-author real read-only full scan made 228 API requests and completed both sources. The final Phase 3A implementation replayed that exact captured input to generate the review export, state sample and behavioral reports without making additional source requests.

Final Linux validation run `33997566400` tested commit `a1260120138f2ef11bdf08673544a68b4a30db3f`; all tests and workflow assertions passed.

Files:

- `review-export.json` / `.csv`: all 215 active review items. Every row includes source identity, original title and author field, author search query, reason, candidate work IDs, deterministic version evidence, whitelisted metadata and fingerprints.
- `review-reason-summary.json` / `.csv`: reason counts.
- `first-real-dry-run-report.json`: original real request metrics and per-author/source boundaries.
- `current-replay-report.json`: first state transition using the final code and captured source input.
- `second-replay-report.json`: identical input replay; zero new events and zero reanalysis.
- `incremental-report.json`: threshold-5 historical-ID early-stop evidence.
- `source-error-report.json`: simulated source error remains partial and does not count as unavailable.
- `sanitized-state-sample.json`: compact examples for catalog, pending, review, latest and scan_state.

This directory does not contain credentials, authorization headers, cookies, raw server responses, image URLs or uploader profiles. Metadata is limited to adapter whitelist fields. It is evidence for Phase 3A only, not production state and not a download queue.
