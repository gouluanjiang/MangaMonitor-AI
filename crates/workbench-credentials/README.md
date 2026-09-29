# Workbench credentials

Native session storage and optional JM login for the MangaMonitor desktop preview. `Vault` accepts only
`Source::Jm` and `Source::Pica`, with one application-owned slot per source.
`WindowsVault::new()` uses the fixed
`MangaMonitor/WorkbenchPreview/v1/Accounts/{JM|Pica}` targets. There is no target
parameter, enumeration, browser storage, filesystem fallback, or separate password
slot. JM's optional login password shares the existing cookie's protected blob.
Pica remains session-only.

The application must obtain consent to keep a session before saving the server's
JM cookie or Pica token, and separate consent to remember a JM login password.
`StoredCredential::new` creates a session-only record. `with_login_password`
consumes that record and validates a JM-only optional password;
`login_password` borrows it only for native authentication. `session_only`
clones the exact account/session without copying the password, suitable for session
restoration and consent withdrawal. `StoredCredential` has no serde implementation.
Account name, session secret and password are redacted in `Debug` and held in
`Zeroizing<String>`; encoded buffers and returned Windows blobs are also
cleared on drop. Zeroization is best effort and does not promise to erase copies
made by callers, serializers, the compiler, or Windows itself.

The complete versioned JSON blob is limited to 2560 bytes, including UTF-8 and
escaping. Oversized records fail with `CREDENTIAL_TOO_LARGE`, without splitting
or fallback. Source/kind mismatches, empty fields, Unicode control characters,
and account names over 513 UTF-16 units are rejected. Valid strings are preserved
exactly. A malformed or future record fails to load and is retained. An explicit
save replaces the application's source slot; deleting a missing slot succeeds.
Passwords must be nonempty and contain no control characters; their leading or
trailing spaces are not trimmed. Version 1 session-only blobs remain readable and
are still written without a password. Only a JM record containing a password uses
version 2; version 1 with a password field, version 2 without a valid JM password,
unknown fields and duplicate fields are rejected. Older binaries cannot read v2;
they report an unsupported schema and retain that slot rather than deleting it.
`compare_exchange` atomically checks the observed `StoredCredential::fingerprint`
and replaces or deletes the slot. A stale observation fails with
`CREDENTIAL_CHANGED`; `None` expects a missing slot. Both conditional and ordinary
writes/deletes share the same lock. Callers must use conditional operations when
an earlier observation grants authority to replace or delete a session.
Session-only fingerprints retain their previous representation. Password-bearing
fingerprints use a separate domain and field lengths, so enabling, changing or
removing only the password is also a changed generation. Cookie/password updates
and password removal each replace a single complete blob under the existing lock.
No split writes or second credential target are introduced.

Windows uses a named mutex scoped to this application's fixed namespace, source,
and the effective Windows user's SID hash. The global namespace coordinates the
same user's processes across desktop sessions. The default Windows token DACL
applies; failure to acquire access remains an error. Acquisition waits at most
five seconds (`VAULT_BUSY`), and an abandoned mutex is acquired before re-reading
the OS record. The kernel releases ownership if its thread/process terminates.
This coordinates cooperating application instances; it cannot make other software
that directly edits the Windows credential store follow the application's lock.

Windows stores generic credentials for the current Windows user with
`CRED_PERSIST_LOCAL_MACHINE`, so the session survives subsequent logins on that
computer. It does not roam to another computer. The public error contains only a
stable code. The optional `test-support` feature exposes `MemoryVault` for
explicit dependency injection; it must never be selected because Windows fails.

Run portable contract tests in CI with `cargo test --locked -p workbench-credentials`.
The actual Windows Credential Manager test is ignored by default and additionally
requires `CI=true` and `GITHUB_ACTIONS=true`. On a disposable Windows runner, run:

```text
cargo test --locked -p workbench-credentials windows_credential_manager_roundtrip_uses_only_random_ci_slots -- --ignored
```

That test creates a random 128-bit CI namespace compiled only into the unit-test
binary. It reads/writes/deletes only its two own slots and cleans them up on exit
or assertion failure. It never reads production slots. It covers missing records,
roundtrip, replacement, source isolation, persistence across vault instances,
malformed-record retention, and idempotent deletion. No real account is needed.
It also checks v1-to-v2 upgrade, password-only conditional writes, atomic removal
back to session-only, and launches the same unit-test binary with the random CI
namespace to prove v2 persists across processes and a separate process cannot
delete a locked slot. These synthetic OS tests do not prove real JM expiration or
automatic login behavior, which belongs to account-service validation and user
acceptance.

Microsoft references:

- [CREDENTIALW capacity, target identity, and persistence](https://learn.microsoft.com/en-us/windows/win32/api/wincred/ns-wincred-credentialw)
- [CredWriteW creation and replacement](https://learn.microsoft.com/en-us/windows/win32/api/wincred/nf-wincred-credwritew)
- [CredReadW allocation ownership and errors](https://learn.microsoft.com/en-us/windows/win32/api/wincred/nf-wincred-credreadw)
- [CredDeleteW exact-target deletion](https://learn.microsoft.com/en-us/windows/win32/api/wincred/nf-wincred-creddeletew)
- [Global and per-session kernel namespaces](https://learn.microsoft.com/en-us/windows/win32/termserv/kernel-object-namespaces)
- [CreateMutexW security and ownership](https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-createmutexw)
- [WaitForSingleObject timeout and abandoned-mutex behavior](https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-waitforsingleobject)
