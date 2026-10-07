#requires -Version 7.0
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$script:BinaryName = 'mangamonitor-workbench-preview.exe'
$script:RegistrationKey = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\MangaMonitor Dev'
$script:PayloadNames = @(
    $script:BinaryName, 'LICENSE.txt', 'THIRD_PARTY_NOTICES.md', 'USER_GUIDE.md',
    'RELEASE_NOTES.md', 'licenses/THIRD_PARTY_LICENSES.txt', 'licenses/inventory.json'
)

function Stop-InstallVerification([string] $Code) {
    $exception = [InvalidOperationException]::new($Code)
    $exception.Data['InstallFailureCode'] = $Code
    throw $exception
}

function Get-InstallFailureCode([Exception] $Exception) {
    # Never expose Message, StackTrace, command lines, environment values or paths.
    while ($null -ne $Exception) {
        $code = $Exception.Data['InstallFailureCode']
        if ($code -is [string] -and $code -cmatch '^INSTALL_[A-Z_]{1,64}$') { return $code }
        $Exception = $Exception.InnerException
    }
    return 'INSTALL_VERIFICATION_UNAVAILABLE'
}

function Get-CheckedWindowsPath([string] $Path) {
    if (-not $IsWindows) { Stop-InstallVerification 'INSTALL_WINDOWS_REQUIRED' }
    # Explicit local DOS paths only: no shell expansion, UNC, device paths, ADS,
    # relative segments, ambiguous trailing dots/spaces, wildcards or links.
    if ($Path -notmatch '^[A-Za-z]:[\\/]' -or $Path -match '[\x00-\x1f"<>|?*]' -or
        $Path.Substring(2).Contains(':')) { Stop-InstallVerification 'INSTALL_PATH_INVALID' }
    $full = [IO.Path]::GetFullPath($Path).TrimEnd('\', '/')
    if ($full.Length -le 3) { Stop-InstallVerification 'INSTALL_PATH_INVALID' }
    foreach ($segment in ($Path.Substring(3).TrimEnd('\', '/') -split '[\\/]')) {
        if (-not $segment -or $segment -in @('.', '..') -or $segment -match '[. ]$') {
            Stop-InstallVerification 'INSTALL_PATH_INVALID'
        }
    }
    $cursor = $full
    while ($cursor) {
        # GetAttributes distinguishes a missing path from denied inspection.
        try {
            $attributes = [IO.File]::GetAttributes($cursor)
            if ($attributes -band [IO.FileAttributes]::ReparsePoint) {
                Stop-InstallVerification 'INSTALL_REPARSE_POINT_REJECTED'
            }
        } catch [IO.FileNotFoundException] {
        } catch [IO.DirectoryNotFoundException] {
        }
        $cursor = [IO.Path]::GetDirectoryName($cursor)
    }
    return $full
}

function Get-InstallFileIdentity([string] $Path) {
    $full = Get-CheckedWindowsPath $Path
    $stream = [IO.File]::Open($full, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try {
        $hash = [Security.Cryptography.SHA256]::Create()
        try {
            return [pscustomobject]@{
                bytes = $stream.Length
                sha256 = [Convert]::ToHexString($hash.ComputeHash($stream)).ToLowerInvariant()
            }
        } finally { $hash.Dispose() }
    } finally { $stream.Dispose() }
}

function Assert-InstallPayloadList([object[]] $Artifacts) {
    if ($Artifacts.Count -ne $script:PayloadNames.Count) { Stop-InstallVerification 'INSTALL_PAYLOAD_LIST_INVALID' }
    $seen = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($entry in $Artifacts) {
        if ($entry.file -cnotin $script:PayloadNames -or -not $seen.Add($entry.file) -or
            $entry.sha256 -cnotmatch '^[0-9a-f]{64}$' -or
            -not ($entry.bytes -is [int] -or $entry.bytes -is [long]) -or $entry.bytes -le 0) {
            Stop-InstallVerification 'INSTALL_PAYLOAD_LIST_INVALID'
        }
    }
}

function Read-VerifiedReleaseManifest([string] $ManifestPath, [string] $ManifestSha256) {
    if ($ManifestSha256 -notmatch '^[0-9a-fA-F]{64}$') { Stop-InstallVerification 'INSTALL_MANIFEST_PIN_REQUIRED' }
    $full = Get-CheckedWindowsPath $ManifestPath
    $stream = [IO.File]::Open($full, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try {
        if ($stream.Length -le 0 -or $stream.Length -gt 1MB) { Stop-InstallVerification 'INSTALL_MANIFEST_INVALID' }
        $hasher = [Security.Cryptography.SHA256]::Create()
        try { $actual = [Convert]::ToHexString($hasher.ComputeHash($stream)) } finally { $hasher.Dispose() }
        if ($actual -ine $ManifestSha256) { Stop-InstallVerification 'INSTALL_MANIFEST_HASH_MISMATCH' }
        $stream.Position = 0
        $reader = [IO.StreamReader]::new($stream, [Text.Encoding]::UTF8, $true, 4096, $true)
        try { $manifest = $reader.ReadToEnd() | ConvertFrom-Json -AsHashtable } finally { $reader.Dispose() }
    } finally { $stream.Dispose() }
    if ($manifest.schemaVersion -ne 1 -or $manifest.productName -cne 'MangaMonitor Dev' -or
        $manifest.identifier -cne 'com.mangamonitor.workbench.preview' -or
        $manifest.version -cnotmatch '^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-rc\.(0|[1-9]\d*))?$' -or
        $manifest.sourceRevision -cnotmatch '^[0-9a-fA-F]{40}$' -or
        $manifest.checkoutRevision -cnotmatch '^[0-9a-fA-F]{40}$' -or
        $manifest.artifacts.Count -ne 8) { Stop-InstallVerification 'INSTALL_MANIFEST_INVALID' }
    $payloads = @($manifest.artifacts | Where-Object { $_.file -cin $script:PayloadNames })
    Assert-InstallPayloadList $payloads
    $installers = @($manifest.artifacts | Where-Object { $_.file -cnotin $script:PayloadNames })
    $names = @("MangaMonitor Dev_$($manifest.version)_x64-setup.exe", "MangaMonitor.Dev_$($manifest.version)_x64-setup.exe")
    if ($installers.Count -ne 1 -or $installers[0].file -cnotin $names -or
        $installers[0].sha256 -cnotmatch '^[0-9a-f]{64}$' -or
        -not ($installers[0].bytes -is [int] -or $installers[0].bytes -is [long]) -or $installers[0].bytes -le 0) {
        Stop-InstallVerification 'INSTALL_MANIFEST_INVALID'
    }
    return [pscustomobject]@{
        version = $manifest.version; sourceRevision = $manifest.sourceRevision
        checkoutRevision = $manifest.checkoutRevision; payloads = $payloads; installer = $installers[0]
    }
}

function Read-InstallRegistrations {
    $views = if ([Environment]::Is64BitOperatingSystem) { @('Registry64', 'Registry32') } else { @('Registry32') }
    foreach ($view in $views) {
        $hive = [Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::CurrentUser, [Microsoft.Win32.RegistryView]$view)
        try {
            $key = $hive.OpenSubKey($script:RegistrationKey, $false)
            if ($null -ne $key) {
                try {
                    $values = @{ view = $view }
                    foreach ($name in @('DisplayVersion', 'MainBinaryName', 'InstallLocation', 'UninstallString')) {
                        $values[$name] = $key.GetValue($name, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
                    }
                    [pscustomobject]$values
                } finally { $key.Dispose() }
            }
        } finally { $hive.Dispose() }
    }
}

function Assert-InstallRegistrationLocation([object[]] $Registrations, [string] $InstallDirectory) {
    foreach ($registration in $Registrations) {
        if ($registration.InstallLocation -isnot [string] -or $registration.MainBinaryName -cne $script:BinaryName) {
            Stop-InstallVerification 'INSTALL_REGISTRATION_IDENTITY_MISMATCH'
        }
        $location = $registration.InstallLocation
        if ($location.StartsWith('"') -and $location.EndsWith('"')) { $location = $location.Substring(1, $location.Length - 2) }
        if ((Get-CheckedWindowsPath $location) -ine $InstallDirectory) {
            Stop-InstallVerification 'INSTALL_REGISTRATION_PATH_MISMATCH'
        }
    }
}

function Assert-VerifiedWindowsInstallation([string] $InstallDirectory, [string] $ExpectedVersion, [object[]] $Artifacts) {
    $directory = Get-CheckedWindowsPath $InstallDirectory
    Assert-InstallPayloadList $Artifacts
    foreach ($artifact in $Artifacts) {
        $target = Join-Path $directory $artifact.file
        if (-not [IO.File]::Exists($target)) { Stop-InstallVerification 'INSTALL_PAYLOAD_MISSING' }
        $actual = Get-InstallFileIdentity $target
        if ($actual.bytes -ne $artifact.bytes -or $actual.sha256 -cne $artifact.sha256) {
            if ($artifact.file -ceq $script:BinaryName) { Stop-InstallVerification 'INSTALL_EXE_HASH_MISMATCH' }
            Stop-InstallVerification 'INSTALL_RESOURCE_HASH_MISMATCH'
        }
    }
    $registrations = @(Read-InstallRegistrations)
    if ($registrations.Count -eq 0) { Stop-InstallVerification 'INSTALL_REGISTRATION_MISSING' }
    Assert-InstallRegistrationLocation $registrations $directory
    $uninstaller = Join-Path $directory 'uninstall.exe'
    foreach ($registration in $registrations) {
        if ($registration.DisplayVersion -cne $ExpectedVersion) { Stop-InstallVerification 'INSTALL_REGISTRATION_VERSION_MISMATCH' }
        if ($registration.UninstallString -ine ('"' + $uninstaller + '"')) {
            Stop-InstallVerification 'INSTALL_UNINSTALL_REGISTRATION_MISMATCH'
        }
    }
    $null = Get-CheckedWindowsPath $uninstaller
    if (-not [IO.File]::Exists($uninstaller)) { Stop-InstallVerification 'INSTALL_UNINSTALLER_MISSING' }
    # Both registry views can expose the same shared key. Every present entry
    # must agree; a conflicting stale view cannot rescue or override the target.
    return [pscustomobject]@{
        exeAtRequestedPath = $true; payloadCount = $Artifacts.Count; payloadHashesMatch = $true
        registryScope = 'HKCU'; registrationViews = @($registrations.view)
        registrationMatches = $true; uninstallerPresent = $true
    }
}

function Assert-InstallerExitCode([AllowNull()] [object] $ExitCode) {
    if ($null -eq $ExitCode -or $ExitCode -isnot [int]) { Stop-InstallVerification 'INSTALL_EXIT_CODE_MISSING' }
    if ($ExitCode -ne 0) { Stop-InstallVerification 'INSTALL_PROCESS_NONZERO' }
}

function Get-SafeTempDiagnostic([AllowNull()] [string] $Path) {
    $diagnostic = [ordered]@{
        configured = -not [string]::IsNullOrEmpty($Path); length = ([string]$Path).Length
        nonAscii = [bool]($Path -match '[^\x00-\x7f]'); exists = $false
        createWriteReadDelete = $false; cleanupSucceeded = $null; failureCode = $null
    }
    $probe = $null
    $created = $false
    try {
        $directory = Get-CheckedWindowsPath $Path
        $diagnostic.exists = [IO.Directory]::Exists($directory)
        if (-not $diagnostic.exists) { Stop-InstallVerification 'INSTALL_TEMP_DIRECTORY_MISSING' }
        $probe = Join-Path $directory ('.mangamonitor-probe-' + [guid]::NewGuid().ToString('N'))
        $stream = [IO.File]::Open($probe, [IO.FileMode]::CreateNew, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
        $created = $true
        try {
            $stream.WriteByte(73); $stream.Flush($true); $stream.Position = 0
            if ($stream.ReadByte() -ne 73) { Stop-InstallVerification 'INSTALL_TEMP_READBACK_FAILED' }
        } finally { $stream.Dispose() }
        [IO.File]::Delete($probe)
        $created = $false
        $diagnostic.cleanupSucceeded = $true
        $diagnostic.createWriteReadDelete = $true
    } catch { $diagnostic.failureCode = Get-InstallFailureCode $_.Exception }
    finally {
        if ($created) {
            try { [IO.File]::Delete($probe); $diagnostic.cleanupSucceeded = $true }
            catch { $diagnostic.cleanupSucceeded = $false }
        }
    }
    return [pscustomobject]$diagnostic
}

function Invoke-VerifiedNsisProcess(
    [string] $InstallerPath, [object] $InstallerArtifact, [string] $InstallDirectory,
    [string] $ProcessTempDirectory, [ValidateRange(1, 3600)] [int] $TimeoutSeconds = 600
) {
    $installer = Get-CheckedWindowsPath $InstallerPath
    $directory = Get-CheckedWindowsPath $InstallDirectory
    $stream = [IO.File]::Open($installer, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    $process = [Diagnostics.Process]::new()
    try {
        $hasher = [Security.Cryptography.SHA256]::Create()
        try { $hash = [Convert]::ToHexString($hasher.ComputeHash($stream)).ToLowerInvariant() } finally { $hasher.Dispose() }
        if ($hash -cne $InstallerArtifact.sha256 -or $stream.Length -ne $InstallerArtifact.bytes) {
            Stop-InstallVerification 'INSTALL_INSTALLER_HASH_MISMATCH'
        }
        $start = [Diagnostics.ProcessStartInfo]::new()
        $start.FileName = $installer
        # NSIS /D must be last and unquoted, including when the path has spaces.
        $start.Arguments = '/S /D=' + $directory
        $start.UseShellExecute = $false
        $start.CreateNoWindow = $true
        $start.RedirectStandardOutput = $true
        $start.RedirectStandardError = $true
        if ($ProcessTempDirectory) {
            $temporary = Get-CheckedWindowsPath $ProcessTempDirectory
            $start.Environment['TEMP'] = $temporary
            $start.Environment['TMP'] = $temporary
        }
        $process.StartInfo = $start
        if (-not $process.Start()) { Stop-InstallVerification 'INSTALL_PROCESS_START_FAILED' }
        # Drain, but never publish raw child output (it may contain private paths).
        $null = $process.StandardOutput.ReadToEndAsync()
        $null = $process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit($TimeoutSeconds * 1000)) { Stop-InstallVerification 'INSTALL_PROCESS_TIMEOUT' }
        return [int]$process.ExitCode
    } finally {
        # Disposing does not kill the process on timeout. Do not clean up files
        # it may still use, silently retry, or terminate unrelated processes.
        $process.Dispose(); $stream.Dispose()
    }
}

Export-ModuleMember -Function Stop-InstallVerification, Get-InstallFailureCode, Get-CheckedWindowsPath,
    Get-InstallFileIdentity, Read-VerifiedReleaseManifest, Read-InstallRegistrations,
    Assert-InstallRegistrationLocation, Assert-VerifiedWindowsInstallation, Assert-InstallerExitCode,
    Get-SafeTempDiagnostic, Invoke-VerifiedNsisProcess
