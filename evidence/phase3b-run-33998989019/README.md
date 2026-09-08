# Phase 3B run 33998989019

This directory contains the sanitized, reviewable evidence from the successful
five-author Linux production-style validation. Raw authentication data, raw API
responses, image URLs, uploader profiles, manga files, and images are excluded.

- `first-real-production-style-report.json`: request traces and first-run result
- `second-replay-report.json`: idempotent replay result
- `recovered-report.json`: final state after source-error checkpoint resume
- `state-diff.json`: first-run state changes; deletion authorization is false
- `author-evidence-summary.json`: deterministic author evidence and review counts
- `request-pagination-summary.json`: request delays and page boundaries
- `review-export.json` / `.csv`: all 215 sanitized review items
- `review-reason-summary.json` / `.csv`: reason distribution
- `commit-recovery.json`: temporary Git remote conflict/recovery proof
