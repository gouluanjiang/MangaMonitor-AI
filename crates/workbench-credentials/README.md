# Workbench credentials

Native session storage for the MangaMonitor desktop preview. `Vault` accepts only
`Source::Jm` and `Source::Pica`, with one application-owned slot per source.
`WindowsVault::new()` uses the fixed
`MangaMonitor/WorkbenchPreview/v1/Accounts/{JM|Pica}` targets. There is no target
parameter, enumeration, browser storage, filesystem fallback, or password kind.

The application must obtain consent to keep a login before saving the server's
JM cookie or Pica token. `StoredCredential` has borrowed native-only accessors and
no serde implementation. Both account name and secret are redacted in `Debug` and
held in `Zeroizing<String>`; encoded buffers and returned Windows blobs are also
cleared on drop. Zeroization is best effort and does not promise to erase copies
made by callers, serializers, the compiler, or Windows itself.

The complete versioned JSON blob is limited to 2560 bytes, including UTF-8 and
escaping. Oversized records fail with `CREDENTIAL_TOO_LARGE`, without splitting
or fallback. Source/kind mismatches, empty fields, Unicode control characters,
and account names over 513 UTF-16 units are rejected. Valid strings are preserved
exactly. A malformed or future record fails to load and is retained. An explicit
save replaces the application's source slot; deleting a missing slot succeeds.
`compare_exchange` atomically checks the observed `StoredCredential::fingerprint`
and replaces or deletes the slot. A stale observation fails with
`CREDENTIAL_CHANGED`; `None` expects a missing slot. Both conditional and ordinary
writes/deletes share the same lock. Callers must use conditional operations when
an earlier observation grants authority to replace or delete a session.

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
It also checks conditional writes and launches the same unit-test binary with the
random CI namespace to verify a separate process cannot delete a locked slot.

Microsoft references:

- [CREDENTIALW capacity, target identity, and persistence](https://learn.microsoft.com/en-us/windows/win32/api/wincred/ns-wincred-credentialw)
- [CredWriteW creation and replacement](https://learn.microsoft.com/en-us/windows/win32/api/wincred/nf-wincred-credwritew)
- [CredReadW allocation ownership and errors](https://learn.microsoft.com/en-us/windows/win32/api/wincred/nf-wincred-credreadw)
- [CredDeleteW exact-target deletion](https://learn.microsoft.com/en-us/windows/win32/api/wincred/nf-wincred-creddeletew)
- [Global and per-session kernel namespaces](https://learn.microsoft.com/en-us/windows/win32/termserv/kernel-object-namespaces)
- [CreateMutexW security and ownership](https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-createmutexw)
- [WaitForSingleObject timeout and abandoned-mutex behavior](https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-waitforsingleobject)
