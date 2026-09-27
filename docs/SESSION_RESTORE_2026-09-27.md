# JM session restoration investigation

The user reported having to enter JM account credentials again after updates or reopening the application. Pica behavior was uncertain. Independent reader windows are already user-accepted; this investigation is separate.

## Evidence established

- Windows credential targets and application data are stable across executable paths and updates. Saved application-owned JM and Pica slots were present.
- Code review found no account logout on application/window close or update, and no account initialization from independent reader windows. Failed restoration retains the stored session and supports an explicit status refresh.
- One bounded profile request per source, using only the application's existing stored session in memory, succeeded. No account names, passwords, cookie/token values or raw response were recorded. No credential mutation, application restart, media access or download was performed by the diagnostic.
- JM's profile response returned the same `s` as the saved AVS, with no replacement AVS cookie. This observation does not establish that JM never rotates or expires sessions.
- The user then fully exited and immediately reopened MangaMonitor and confirmed that JM remained connected. The prior error text is unavailable, so the earlier failures cannot yet be attributed conclusively to server expiry or a temporary restore failure.
- The pinned upstream JM downloader signs in again with stored username/password on startup. That differs from our current session-only persistence and does not demonstrate that an AVS token remains valid indefinitely.

## Approved change

The user explicitly accepted optional JM "remember login" on 2026-09-27. It retains login material in Windows Credential Manager, reuses an existing session first, and attempts one reauthentication only after an explicit authentication-expired result. This occurs on application account restoration or explicit account refresh, not by replaying a failed favorite, author scan or download. Network errors, access denial, rate limits and malformed responses do not trigger relogin. Forget/logout removes both session and login material. Pica keeps session-only behavior. This supersedes the prior no-password-persistence contract only for explicitly opted-in JM login.

Existing saved sessions remain readable and do not silently opt in. The user must sign in once in the new version and select the new option. Choosing it also selects session persistence; turning session persistence off turns remembered login off. Summaries expose only a boolean setting, never the saved password. Both session and optional password share the existing fixed Windows slot and its compare-and-exchange transaction. The whole envelope remains subject to the Windows size limit; failed writes preserve existing data and have no plaintext fallback. Old version-1 session records remain compatible; opt-in records use a version-2 envelope.

No new manga/download authority is granted. Existing account generation and session leases still gate all requests and pending plans; a replaced or forgotten credential cannot be resurrected by a delayed login response. Native login material is not handed to reader/image/download sessions. Program closure does not add any background job or login.

## Verification plan

Use synthetic accounts to verify old-session compatibility, password redaction, whole-envelope bounds, explicit JM opt-in/Pica rejection, ordinary restore without reauthentication, one expired-session fallback, errors without repeated login, concurrent restoration and credential replacement, and forgetting the combined credential. Windows CI uses only its random test slots. Browser checks cover the setting, dependencies, source switching and saved-state text; native command checks cover optional-argument compatibility and invalid persistence combinations.

Implementation and an independent read-only review are complete; CI and Dev delivery are pending. Thirteen new account-service scenarios cover explicit-expiry fallback, non-authentication failures, concurrent restoration/replacement, opt-out and non-replay. Credential tests include a child process reading the synthetic combined login record from a random Windows test slot. Two new frontend logic checks and two browser cases cover the new setting and expose only its boolean state. Formal suites/builds run only in CI.

The user-confirmed immediate restart is current evidence, not acceptance of the new fallback. The original historical failures remain unclassified; this change supplies the requested expired-session convenience without asserting that all previous failures were expiry. A version-2 opted-in record is intentionally not readable by earlier binaries; returning to an older Dev requires signing in again there. Existing version-1 records are unchanged until an explicit new login choice.
