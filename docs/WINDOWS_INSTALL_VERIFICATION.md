# Fail-closed Windows delivery verification

## Audit and scope (2026-10-05)

The published 1.0.2 assets and tag are immutable for this maintenance change.
The facts and unresolved boundary in `RELEASE_1.0.2_2026-10-03.md` remain unchanged:
NSIS returned 2 before extraction, then returned 0 with a process-local temporary
directory but did not replace the old executable/registration. A write probe
succeeded; neither it nor the later return code determines the environment cause.
Do not describe this change as a repair of that extraction failure.

`apps/local-workbench/package.json` pins Tauri CLI 2.11.4; `tauri.conf.json`
selects currentUser NSIS and downloadBootstrapper. No dependency, installation
mode, application ID, credential namespace, data directory or security setting
is changed here. WebView2 downloading remains a separate environment dependency;
there is no evidence establishing it, TEMP permissions, antivirus, Unicode,
path length or an NSIS defect as the cause of the recorded failure.

The existing desktop workflow already checked installed bytes, registry values,
shortcuts and installed WebView/restart/data retention before staging a candidate.
Its NSIS binary verifier allows only the pinned bundle-marker transformation.
The missing boundary was a reusable local delivery gate and explicit negative
regressions, not a complete absence of CI installation checks.

## Entry point

Use PowerShell 7 or newer from a reviewed checkout. The default is read-only
installation verification (apart from an explicitly requested JSON report):

```powershell
$releaseDirectory = Read-Host 'Directory containing the verified release manifest'
$installDirectory = Read-Host 'Previously confirmed current-user installation directory'
$reportPath = Join-Path $PWD ('install-verification-' + [guid]::NewGuid().ToString('N') + '.json')
& ./apps/local-workbench/tools/verify-windows-install.ps1 `
  -ManifestPath (Join-Path $releaseDirectory 'manifest.json') `
  -ManifestSha256 '2fa09c08fe9b35ac2d9afd79de8103313f541e7d6d28f5f210772de9693cf5d2' `
  -InstallDirectory $installDirectory -ReportPath $reportPath
if ($LASTEXITCODE -ne 0) { throw 'Installation is not verified.' }
```

That pin is ONLY the published 1.0.2 manifest recorded in the release document.
For a different candidate obtain its expected manifest hash from a separately
reviewed release/CI handoff. Hashing an untrusted downloaded manifest and treating
that same hash as trusted is not authentication. The script does not fetch a
mutable `latest` target or infer expectations from the installed EXE/registry.
The source revision and checkout revision remain separate manifest identities.

For an explicitly authorized install, add `-RunInstaller -InstallerPath $setup`.
The installer can have a different download filename, but its exact length and
SHA-256 must match the single installer entry in the pinned manifest. Back up the
existing installation and data separately and exit the app first. The script
refuses a running app and a registration pointing at another installation; it
never stops the app, edits registration to make a check pass, or copies payloads
as an automatic fallback. It uses silent `/S` and a final, unquoted `/D=` argument.
No `ExecutionPolicy` bypass, `Unblock-File`, elevation, ACL change or security
product exclusion is performed or recommended.

An optional `-ProcessTempDirectory $approvedExistingDirectory` affects only the
new installer process and its children. It does not set user/machine/global
TEMP/TMP, automatically retry the installer, or claim the override fixes NSIS.
The default timeout is 600 seconds (configurable 1–3600). Timeout is failure;
the script does not kill unrelated processes or remove a directory potentially
still in use. A root-process exit is only an observation, never proof that all
installer descendants completed. Incomplete/late changes fail the subsequent
checks; there is no optimistic success or unbounded wait.

## Success contract

Exit 0 and `passed=true` require all of the following in this invocation:

1. The externally pinned manifest hash, supported schema and application identity
   are valid; all seven expected payload paths are unique and allowlisted.
2. The actual EXE at the explicitly requested local directory and the six bundled
   resources match the manifest lengths and SHA-256 hashes. Version equality
   alone cannot accept an old binary from the same version or another commit.
3. At least one current-user uninstall registration exists; every visible HKCU
   registry view agrees on version, main executable name and canonical location.
   The quoted uninstall command points to the target's existing uninstaller.
4. In install mode, the pinned installer actually exits with integer code 0.
   Missing code, nonzero code (including reboot-related codes), launch failure,
   timeout, missing/denied reads and malformed metadata are failures.

Relative/UNC/device/ADS paths, ambiguous components and reparse points are
rejected rather than resolved into a different installation. The uninstaller is
checked for presence/location, not a release hash: it is generated by NSIS and
is not one of the seven manifest payloads. This does not certify uninstall code,
a desktop shortcut, a running process, UI acceptance, or protection against
hostile concurrent filesystem mutation. Existing CI retains its independent
shortcut and native WebView checks. A matching preexisting install can be
verified; the result does not claim bytes changed or an upgrade occurred.

Reports contain fixed failure codes, stage, process exit code, public manifest
identities and booleans/counts. TEMP/TMP diagnostics contain configured/existing
flags, lengths, non-ASCII flags and create/write/read/delete probe outcomes, not
path strings, usernames, environment dumps, exception messages or child output.
Only a randomly named synthetic probe is removed. Reports reserve a new filename
with `passed=false`, then publish a complete result. Existing report paths are
refused: consumers must require this invocation's exit status and attemptId and
must not reuse an older report. Failure to publish the report also returns failure.

## Automated evidence and diagnostic boundaries

`windows-install-verification.yml` parses the scripts and runs the synthetic
Windows contract once on a disposable hosted runner. It covers manifest trust,
missing/changed bytes, same-version stale EXE, stale/missing/misdirected HKCU
registration, missing/misdirected uninstaller, reparse points, a real no-op
process returning 0 with old state, return code 2, timeout, process-only TEMP/TMP,
Unicode/spaced temp paths and path-free output. Test registry/files are generated
only after refusing existing app state; real profiles and credentials are not used.

The desktop installer lifecycle additionally calls the same post-install gate.
After its existing same-version reinstall test it deliberately makes ONLY its
disposable EXE/registration stale, requires rejection, runs the current NSIS
installer, then requires restored hashes/registration and retained synthetic data.
This is synthetic stale-target replacement, not a historical-version migration
or reproduction of the original machine's extraction failure. The native suite
still runs once, and candidate staging remains after all installation gates.

Run results must be attached to the actual commit/CI run. Adding these tests does
not make an unexecuted or pending run a PASS. No real-machine NSIS diagnosis,
interactive/security-prompt matrix, previously waived tests, cloud monitoring,
production actions, release replacement, merge or user installation is authorized
by this change.

References: the repository's `scripts/verify-nsis-bundle.mjs`,
`apps/local-workbench/tests/installer-smoke.ps1` and dated 1.0.2 delivery record;
Tauri Windows installer guide: https://v2.tauri.app/distribute/windows-installer/ ;
NSIS command-line contract: https://nsis.sourceforge.io/Docs/Chapter3.html .
