#requires -Version 7.0
# Default is verification only. Installation requires the explicit -RunInstaller switch.
[CmdletBinding(DefaultParameterSetName = 'Verify')]
param(
    [Parameter(Mandatory)] [string] $ManifestPath,
    [Parameter(Mandatory)] [string] $ManifestSha256,
    [Parameter(Mandatory)] [string] $InstallDirectory,
    [string] $ReportPath,
    [Parameter(Mandatory, ParameterSetName = 'Install')] [switch] $RunInstaller,
    [Parameter(Mandatory, ParameterSetName = 'Install')] [string] $InstallerPath,
    [Parameter(ParameterSetName = 'Install')] [string] $ProcessTempDirectory,
    [Parameter(ParameterSetName = 'Install')] [ValidateRange(1, 3600)] [int] $TimeoutSeconds = 600
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$evidence = [ordered]@{
    schemaVersion = 1; attemptId = [guid]::NewGuid().ToString('N')
    mode = $(if ($RunInstaller) { 'install-and-verify' } else { 'verify-only' })
    passed = $false; stage = 'preflight'; failureCode = $null; installerExitCode = $null
    manifestSha256 = $null; version = $null; sourceRevisionFromManifest = $null
    checkoutRevisionFromManifest = $null; preexistingExecutableMatchesRelease = $null
    temp = $null; verification = $null
    diagnosticBoundary = 'A write probe does not reproduce NSIS extraction; no local root cause is asserted.'
}
$ownedReport = $null
function Write-ReportBytes([IO.Stream] $Stream, [string] $Text) {
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes($Text)
    $Stream.Write($bytes, 0, $bytes.Length)
    $Stream.Flush($true)
}
function Publish-Report([string] $Path, [string] $Text) {
    $temporary = Join-Path ([IO.Path]::GetDirectoryName($Path)) ('.mm-report-' + [guid]::NewGuid().ToString('N'))
    try {
        $stream = [IO.File]::Open($temporary, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
        try { Write-ReportBytes $stream $Text } finally { $stream.Dispose() }
        $null = Get-CheckedWindowsPath $Path
        [IO.File]::Move($temporary, $Path, $true)
    } finally {
        if ([IO.File]::Exists($temporary)) { [IO.File]::Delete($temporary) }
    }
}
try {
    Import-Module (Join-Path $PSScriptRoot 'windows-install-verification.psm1') -Force -DisableNameChecking
    $directory = Get-CheckedWindowsPath $InstallDirectory
    $manifestFullPath = Get-CheckedWindowsPath $ManifestPath
    if ($manifestFullPath.StartsWith($directory + '\', [StringComparison]::OrdinalIgnoreCase)) {
        Stop-InstallVerification 'INSTALL_MANIFEST_INSIDE_TARGET'
    }
    if ($ReportPath) {
        $report = Get-CheckedWindowsPath $ReportPath
        if ($report -ieq $manifestFullPath -or $report.StartsWith($directory + '\', [StringComparison]::OrdinalIgnoreCase) -or
            ($InstallerPath -and $report -ieq (Get-CheckedWindowsPath $InstallerPath))) {
            Stop-InstallVerification 'INSTALL_REPORT_PATH_CONFLICT'
        }
        # Refuse report reuse; consumers must require this invocation's exit code and attemptId.
        $stream = [IO.File]::Open($report, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
        $ownedReport = $report
        try { Write-ReportBytes $stream ($evidence | ConvertTo-Json -Depth 8) } finally { $stream.Dispose() }
    }
    $release = Read-VerifiedReleaseManifest $manifestFullPath $ManifestSha256
    $evidence.manifestSha256 = $ManifestSha256.ToLowerInvariant()
    $evidence.version = $release.version
    $evidence.sourceRevisionFromManifest = $release.sourceRevision
    $evidence.checkoutRevisionFromManifest = $release.checkoutRevision
    if ($RunInstaller) {
        $installerFullPath = Get-CheckedWindowsPath $InstallerPath
        if ($installerFullPath.StartsWith($directory + '\', [StringComparison]::OrdinalIgnoreCase)) {
            Stop-InstallVerification 'INSTALL_INSTALLER_INSIDE_TARGET'
        }
        Assert-InstallRegistrationLocation @(Read-InstallRegistrations) $directory
        if (Get-Process -Name 'mangamonitor-workbench-preview' -ErrorAction SilentlyContinue) {
            Stop-InstallVerification 'INSTALL_APPLICATION_RUNNING'
        }
        $exe = Join-Path $directory 'mangamonitor-workbench-preview.exe'
        if ([IO.File]::Exists($exe)) {
            $before = Get-InstallFileIdentity $exe
            $expected = @($release.payloads | Where-Object { $_.file -ceq 'mangamonitor-workbench-preview.exe' })[0]
            $evidence.preexistingExecutableMatchesRelease = $before.sha256 -ceq $expected.sha256 -and $before.bytes -eq $expected.bytes
        } else { $evidence.preexistingExecutableMatchesRelease = $false }
        $evidence.stage = 'temporary-directory-probe'
        $effective = [IO.Path]::GetTempPath()
        $selected = if ($ProcessTempDirectory) { Get-CheckedWindowsPath $ProcessTempDirectory } else { $effective }
        $evidence.temp = [ordered]@{
            inheritedTEMP = Get-SafeTempDiagnostic $env:TEMP
            inheritedTMP = Get-SafeTempDiagnostic $env:TMP
            selectedSource = $(if ($ProcessTempDirectory) { 'process-only-override' } else { 'inherited-effective' })
            selected = Get-SafeTempDiagnostic $selected
        }
        if (-not $evidence.temp.selected.createWriteReadDelete) { Stop-InstallVerification 'INSTALL_TEMP_PROBE_FAILED' }
        $evidence.stage = 'installer-process'
        $evidence.installerExitCode = Invoke-VerifiedNsisProcess -InstallerPath $installerFullPath -InstallerArtifact $release.installer `
            -InstallDirectory $directory -ProcessTempDirectory $ProcessTempDirectory -TimeoutSeconds $TimeoutSeconds
        Assert-InstallerExitCode $evidence.installerExitCode
    }
    $evidence.stage = 'post-install-verification'
    $evidence.verification = Assert-VerifiedWindowsInstallation -InstallDirectory $directory -ExpectedVersion $release.version -Artifacts $release.payloads
    $evidence.passed = $true
    $evidence.stage = 'verified'
} catch {
    $evidence.passed = $false
    if (Get-Command Get-InstallFailureCode -ErrorAction SilentlyContinue) {
        $evidence.failureCode = Get-InstallFailureCode $_.Exception
    } else { $evidence.failureCode = 'INSTALL_VERIFICATION_UNAVAILABLE' }
}
if ($ownedReport) {
    try { Publish-Report $ownedReport ($evidence | ConvertTo-Json -Depth 8) }
    catch {
        $evidence.passed = $false; $evidence.stage = 'report-publication'
        $evidence.failureCode = 'INSTALL_REPORT_WRITE_FAILED'
    }
}
# A nonzero process result, missing evidence or any read/parse/report failure is
# failure, never an assumed install. No registry repair or payload fallback here.
$evidence | ConvertTo-Json -Depth 8
if (-not $evidence.passed) { exit 1 }
exit 0
