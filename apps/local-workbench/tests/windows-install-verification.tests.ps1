#requires -Version 7.0
# Synthetic files and a no-op process test the observation contract, NOT the
# cause of the 2026-10-03 local NSIS extraction failure.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not $IsWindows -or $env:CI -cne 'true' -or $env:GITHUB_ACTIONS -cne 'true' -or
    $env:RUNNER_ENVIRONMENT -cne 'github-hosted') { throw 'Disposable GitHub-hosted Windows runner required.' }
Import-Module (Join-Path $PSScriptRoot '../tools/windows-install-verification.psm1') -Force -DisableNameChecking
if (@(Read-InstallRegistrations).Count -ne 0 -or
    (Get-Process -Name 'mangamonitor-workbench-preview' -ErrorAction SilentlyContinue) -or
    (Test-Path -LiteralPath (Join-Path ([Environment]::GetFolderPath('ApplicationData')) 'com.mangamonitor.workbench.preview'))) {
    throw 'Refusing an existing application or user profile.'
}
$root = Join-Path $env:RUNNER_TEMP ('Private Sentinel 中文 ' + [guid]::NewGuid().ToString('N'))
$directory = Join-Path $root 'Installed Application'
$childTemp = Join-Path $root 'Child Temp 中文'
[IO.Directory]::CreateDirectory($directory) | Out-Null
[IO.Directory]::CreateDirectory($childTemp) | Out-Null
$binary = 'mangamonitor-workbench-preview.exe'
$names = @($binary, 'LICENSE.txt', 'THIRD_PARTY_NOTICES.md', 'USER_GUIDE.md', 'RELEASE_NOTES.md', 'licenses/THIRD_PARTY_LICENSES.txt', 'licenses/inventory.json')
$ownedRegistration = $false
$registrationKey = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\MangaMonitor Dev'
$results = [Collections.Generic.List[object]]::new()
$savedCapture = $env:MM_TEST_CONTEXT_FILE
$savedExit = $env:MM_TEST_PROCESS_EXIT
$savedDelay = $env:MM_TEST_PROCESS_DELAY
function Assert-Test([bool] $Condition) { if (-not $Condition) { throw 'Synthetic assertion failed.' } }
function Check([string] $Name, [string] $ExpectedCode, [scriptblock] $Action) {
    $observed = ''
    try { $null = & $Action } catch { $observed = Get-InstallFailureCode $_.Exception }
    $passed = $observed -ceq $ExpectedCode
    $results.Add([pscustomobject]@{ name = $Name; passed = $passed; expected = $ExpectedCode; observed = $observed })
    Write-Output ("{0}: {1}" -f $(if ($passed) { 'PASS' } else { 'FAIL' }), $Name)
}
function Restore-Payloads {
    foreach ($name in $names) {
        $file = Join-Path $directory $name
        [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($file)) | Out-Null
        [IO.File]::WriteAllText($file, 'synthetic-new-release:' + $name, [Text.UTF8Encoding]::new($false))
    }
    [IO.File]::WriteAllText((Join-Path $directory 'uninstall.exe'), 'synthetic-uninstaller; never executed')
}
function Set-Registration([string] $Version = '1.0.2', [string] $Location = $directory,
    [string] $BinaryName = $binary, [string] $Uninstall = ('"' + (Join-Path $directory 'uninstall.exe') + '"')) {
    foreach ($view in @('Registry64', 'Registry32')) {
        $hive = [Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::CurrentUser, [Microsoft.Win32.RegistryView]$view)
        try {
            $key = $hive.CreateSubKey($registrationKey)
            $script:ownedRegistration = $true
            try {
                $key.SetValue('DisplayVersion', $Version)
                $key.SetValue('InstallLocation', $Location)
                $key.SetValue('MainBinaryName', $BinaryName)
                $key.SetValue('UninstallString', $Uninstall)
            } finally { $key.Dispose() }
        } finally { $hive.Dispose() }
    }
}
function Verify-Fixture { Assert-VerifiedWindowsInstallation $directory '1.0.2' $payloads }
function Save-Manifest([object] $Value, [string] $Path) {
    [IO.File]::WriteAllText($Path, ($Value | ConvertTo-Json -Depth 8), [Text.UTF8Encoding]::new($false))
    return (Get-InstallFileIdentity $Path).sha256
}
function Check-Cli([string[]] $Extra, [int] $ExpectedExit, [string] $Failure) {
    $reportPath = Join-Path $root ([guid]::NewGuid().ToString('N') + '.json')
    $text = & $shellPath -NoProfile -File $cli -ManifestPath $manifestPath -ManifestSha256 $pin `
        -InstallDirectory $directory -ReportPath $reportPath @Extra 2>$null
    $exitCode = $LASTEXITCODE
    $joined = $text -join "`n"
    Assert-Test (-not $joined.Contains($root) -and -not $joined.Contains('Private Sentinel') -and -not $joined.Contains($env:USERPROFILE))
    $result = $joined | ConvertFrom-Json -AsHashtable
    $report = Get-Content -LiteralPath $reportPath -Raw | ConvertFrom-Json -AsHashtable
    Assert-Test ($exitCode -eq $ExpectedExit -and $result.passed -eq ($ExpectedExit -eq 0))
    Assert-Test ([string]$result.failureCode -ceq $Failure -and $report.attemptId -ceq $result.attemptId -and $report.passed -eq $result.passed)
    return $result
}
try {
    Restore-Payloads
    $payloads = @($names | ForEach-Object {
        $identity = Get-InstallFileIdentity (Join-Path $directory $_)
        @{ file = $_; sha256 = $identity.sha256; bytes = [long]$identity.bytes }
    })
    # A tiny, known no-op EXE makes process exit 0/2 and process-only TEMP/TMP
    # reproducible without downloading historical installers or touching data.
    $stub = Join-Path $root 'Synthetic Setup.exe'
    $source = Join-Path $root 'SyntheticSetup.cs'
    @'
using System;
using System.IO;
using System.Threading;
class SyntheticSetup {
    static int Main() {
        string capture = Environment.GetEnvironmentVariable("MM_TEST_CONTEXT_FILE");
        if (!String.IsNullOrEmpty(capture)) File.WriteAllText(capture,
            Environment.GetEnvironmentVariable("TEMP") + "\n" +
            Environment.GetEnvironmentVariable("TMP") + "\n" + Environment.CommandLine);
        Console.WriteLine("Private child output: " + Environment.GetEnvironmentVariable("USERPROFILE"));
        Console.Error.WriteLine("Private child error: " + Environment.GetEnvironmentVariable("TEMP"));
        Thread.Sleep(Int32.Parse(Environment.GetEnvironmentVariable("MM_TEST_PROCESS_DELAY") ?? "0"));
        return Int32.Parse(Environment.GetEnvironmentVariable("MM_TEST_PROCESS_EXIT") ?? "0");
    }
}
'@ | Set-Content -LiteralPath $source -Encoding utf8
    $compiler = Join-Path $env:WINDIR 'Microsoft.NET/Framework64/v4.0.30319/csc.exe'
    $null = & $compiler /nologo /target:exe "/out:$stub" $source 2>$null
    Assert-Test ($LASTEXITCODE -eq 0)
    $stubIdentity = Get-InstallFileIdentity $stub
    $installerArtifact = @{ file = 'MangaMonitor Dev_1.0.2_x64-setup.exe'; sha256 = $stubIdentity.sha256; bytes = [long]$stubIdentity.bytes }
    $manifest = @{
        schemaVersion = 1; productName = 'MangaMonitor Dev'; identifier = 'com.mangamonitor.workbench.preview'
        version = '1.0.2'; sourceRevision = ('a' * 40); checkoutRevision = ('b' * 40)
        artifacts = @($payloads) + @($installerArtifact)
    }
    $manifestPath = Join-Path $root 'manifest.json'
    $pin = Save-Manifest $manifest $manifestPath
    $cli = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../tools/verify-windows-install.ps1'))
    $shellPath = (Get-Process -Id $PID).Path
    $env:MM_TEST_PROCESS_EXIT = '0'
    $env:MM_TEST_PROCESS_DELAY = '0'
    $env:MM_TEST_CONTEXT_FILE = Join-Path $root 'private-child-context.txt'

    Check 'trusted manifest preserves separate source and checkout identities' '' {
        $release = Read-VerifiedReleaseManifest $manifestPath $pin
        Assert-Test ($release.sourceRevision -ceq ('a' * 40) -and $release.checkoutRevision -ceq ('b' * 40))
    }
    Check 'missing manifest pin is refused' 'INSTALL_MANIFEST_PIN_REQUIRED' { Read-VerifiedReleaseManifest $manifestPath '' }
    Check 'wrong manifest pin is refused' 'INSTALL_MANIFEST_HASH_MISMATCH' { Read-VerifiedReleaseManifest $manifestPath ('0' * 64) }
    Check 'tampered manifest is refused before interpretation' 'INSTALL_MANIFEST_HASH_MISMATCH' {
        [IO.File]::AppendAllText($manifestPath, ' ')
        try { Read-VerifiedReleaseManifest $manifestPath $pin } finally { $null = Save-Manifest $manifest $manifestPath }
    }
    Check 'manifest traversal cannot escape the target' 'INSTALL_PAYLOAD_LIST_INVALID' {
        $copy = ($manifest | ConvertTo-Json -Depth 8) | ConvertFrom-Json -AsHashtable
        $copy.artifacts[0].file = '../outside.exe'
        $other = Join-Path $root 'bad-manifest.json'
        Read-VerifiedReleaseManifest $other (Save-Manifest $copy $other)
    }
    Check 'duplicate manifest payload is refused' 'INSTALL_PAYLOAD_LIST_INVALID' {
        $copy = ($manifest | ConvertTo-Json -Depth 8) | ConvertFrom-Json -AsHashtable
        $copy.artifacts[1].file = $binary
        $other = Join-Path $root 'duplicate-manifest.json'
        Read-VerifiedReleaseManifest $other (Save-Manifest $copy $other)
    }
    Check 'relative destination is refused' 'INSTALL_PATH_INVALID' { Get-CheckedWindowsPath 'relative/path' }
    Check 'missing exit code is not coerced to success' 'INSTALL_EXIT_CODE_MISSING' { Assert-InstallerExitCode $null }
    Check 'nonzero exit remains failure' 'INSTALL_PROCESS_NONZERO' { Assert-InstallerExitCode ([int]2) }
    Check 'correct payloads without registration are not installed' 'INSTALL_REGISTRATION_MISSING' { Verify-Fixture }
    Set-Registration
    Check 'matching payloads and current-user registration pass' '' { Verify-Fixture }
    Check 'same-version stale executable fails by hash' 'INSTALL_EXE_HASH_MISMATCH' {
        [IO.File]::WriteAllText((Join-Path $directory $binary), 'old same-version binary')
        try { Verify-Fixture } finally { Restore-Payloads }
    }
    Check 'missing executable fails' 'INSTALL_PAYLOAD_MISSING' {
        [IO.File]::Delete((Join-Path $directory $binary))
        try { Verify-Fixture } finally { Restore-Payloads }
    }
    Check 'missing resource fails' 'INSTALL_PAYLOAD_MISSING' {
        [IO.File]::Delete((Join-Path $directory 'LICENSE.txt'))
        try { Verify-Fixture } finally { Restore-Payloads }
    }
    Check 'modified resource fails' 'INSTALL_RESOURCE_HASH_MISMATCH' {
        [IO.File]::AppendAllText((Join-Path $directory 'LICENSE.txt'), 'changed')
        try { Verify-Fixture } finally { Restore-Payloads }
    }
    Check 'stale registry version fails even with correct bytes' 'INSTALL_REGISTRATION_VERSION_MISMATCH' {
        Set-Registration -Version '1.0.1'
        try { Verify-Fixture } finally { Set-Registration }
    }
    Check 'registration pointing to another installation fails' 'INSTALL_REGISTRATION_PATH_MISMATCH' {
        Set-Registration -Location (Join-Path $root 'Other Application')
        try { Verify-Fixture } finally { Set-Registration }
    }
    Check 'wrong registered executable identity fails' 'INSTALL_REGISTRATION_IDENTITY_MISMATCH' {
        Set-Registration -BinaryName 'other.exe'
        try { Verify-Fixture } finally { Set-Registration }
    }
    Check 'uninstall command outside the target fails' 'INSTALL_UNINSTALL_REGISTRATION_MISMATCH' {
        Set-Registration -Uninstall '"C:\outside\uninstall.exe"'
        try { Verify-Fixture } finally { Set-Registration }
    }
    Check 'missing uninstaller fails' 'INSTALL_UNINSTALLER_MISSING' {
        [IO.File]::Delete((Join-Path $directory 'uninstall.exe'))
        try { Verify-Fixture } finally { Restore-Payloads }
    }
    Check 'junction destination is refused' 'INSTALL_REPARSE_POINT_REJECTED' {
        $junction = Join-Path $root 'Redirected Application'
        $null = New-Item -ItemType Junction -Path $junction -Target $directory
        try { Get-CheckedWindowsPath $junction } finally { [IO.Directory]::Delete($junction) }
    }
    Check 'temp diagnostics redact spaced and Unicode paths' '' {
        $diagnostic = Get-SafeTempDiagnostic $childTemp
        $text = $diagnostic | ConvertTo-Json
        Assert-Test ($diagnostic.createWriteReadDelete -and $diagnostic.nonAscii -and -not $text.Contains($root) -and -not $text.Contains('Private Sentinel'))
    }
    Check 'missing temp directory produces safe failure context' '' {
        $diagnostic = Get-SafeTempDiagnostic (Join-Path $root 'not-created')
        Assert-Test (-not $diagnostic.createWriteReadDelete -and $diagnostic.failureCode -ceq 'INSTALL_TEMP_DIRECTORY_MISSING')
    }
    Check 'unset temp value produces safe failure context' '' {
        $diagnostic = Get-SafeTempDiagnostic $null
        Assert-Test (-not $diagnostic.configured -and -not $diagnostic.createWriteReadDelete)
    }
    Check 'process TEMP override leaves the parent unchanged and /D last' '' {
        $tempBefore = $env:TEMP; $tmpBefore = $env:TMP
        $exitCode = Invoke-VerifiedNsisProcess $stub $installerArtifact $directory $childTemp 10
        $context = [IO.File]::ReadAllLines($env:MM_TEST_CONTEXT_FILE)
        Assert-Test ($exitCode -eq 0 -and $env:TEMP -ceq $tempBefore -and $env:TMP -ceq $tmpBefore)
        Assert-Test ($context[0] -ceq $childTemp -and $context[1] -ceq $childTemp -and $context[2].EndsWith('/S /D=' + $directory))
    }
    Check 'CLI verify-only independently confirms installed bytes and registration' '' { Check-Cli @() 0 '' }
    Check 'real process exit zero with unchanged old EXE is not success' '' {
        [IO.File]::WriteAllText((Join-Path $directory $binary), 'unchanged old installation')
        try {
            $result = Check-Cli @('-RunInstaller', '-InstallerPath', $stub, '-ProcessTempDirectory', $childTemp) 1 'INSTALL_EXE_HASH_MISMATCH'
            Assert-Test ($result.installerExitCode -eq 0 -and -not $result.preexistingExecutableMatchesRelease)
        } finally { Restore-Payloads }
    }
    Check 'real process exit zero with stale registration is not success' '' {
        Set-Registration -Version '1.0.1'
        try { Check-Cli @('-RunInstaller', '-InstallerPath', $stub) 1 'INSTALL_REGISTRATION_VERSION_MISMATCH' }
        finally { Set-Registration }
    }
    Check 'real process exit two is retained even when installed bytes already match' '' {
        $env:MM_TEST_PROCESS_EXIT = '2'
        try {
            $result = Check-Cli @('-RunInstaller', '-InstallerPath', $stub) 1 'INSTALL_PROCESS_NONZERO'
            Assert-Test ($result.installerExitCode -eq 2)
        } finally { $env:MM_TEST_PROCESS_EXIT = '0' }
    }
    Check 'installer bytes are checked before launch' '' {
        $different = Join-Path $root 'Changed Setup.exe'
        [IO.File]::WriteAllText($different, 'not the trusted installer')
        Check-Cli @('-RunInstaller', '-InstallerPath', $different) 1 'INSTALL_INSTALLER_HASH_MISMATCH'
    }
    Check 'invalid process temp never launches the installer' '' {
        $result = Check-Cli @('-RunInstaller', '-InstallerPath', $stub, '-ProcessTempDirectory', (Join-Path $root 'not-created')) 1 'INSTALL_TEMP_PROBE_FAILED'
        Assert-Test ($null -eq $result.installerExitCode)
    }
    Check 'timeout cannot be reported as installation success' '' {
        $env:MM_TEST_PROCESS_DELAY = '2000'
        try { Check-Cli @('-RunInstaller', '-InstallerPath', $stub, '-TimeoutSeconds', '1') 1 'INSTALL_PROCESS_TIMEOUT' }
        finally { Start-Sleep -Seconds 3; $env:MM_TEST_PROCESS_DELAY = '0' }
    }
} finally {
    $env:MM_TEST_CONTEXT_FILE = $savedCapture
    $env:MM_TEST_PROCESS_EXIT = $savedExit
    $env:MM_TEST_PROCESS_DELAY = $savedDelay
    if ($ownedRegistration) {
        foreach ($view in @('Registry64', 'Registry32')) {
            $hive = [Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::CurrentUser, [Microsoft.Win32.RegistryView]$view)
            try { $hive.DeleteSubKeyTree($registrationKey, $false) } finally { $hive.Dispose() }
        }
    }
    # Only the unique directory created above; no profile, shortcut or library.
    [IO.Directory]::Delete($root, $true)
}
$failed = @($results | Where-Object { -not $_.passed })
Write-Output ("WINDOWS_INSTALL_CONTRACT: {0} passed; {1} failed; synthetic environment only." -f ($results.Count - $failed.Count), $failed.Count)
if ($failed.Count -ne 0) { throw 'Windows install verification contract regression failed.' }
