# Run only on a fresh GitHub-hosted Windows runner. This verifies the current
# release package, not migration from an unavailable historical 0.3.4 installer.
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not $IsWindows -or $env:CI -cne 'true' -or $env:GITHUB_ACTIONS -cne 'true' -or
    $env:RUNNER_ENVIRONMENT -cne 'github-hosted') {
    throw 'Installer smoke requires a disposable GitHub-hosted Windows runner.'
}
if ($env:MANGAMONITOR_BUILD_REVISION -cnotmatch '^[0-9a-fA-F]{40}$' -or
    $env:GITHUB_SHA -cnotmatch '^[0-9a-fA-F]{40}$') {
    throw 'Release evidence requires exact source and checkout revisions.'
}
$appDirectory = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$repository = [IO.Path]::GetFullPath((Join-Path $appDirectory '../..'))
$config = Get-Content -LiteralPath (Join-Path $appDirectory 'src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json
if ($config.identifier -cne 'com.mangamonitor.workbench.preview' -or
    $config.productName -cne 'MangaMonitor Dev' -or $config.bundle.windows.nsis.installMode -cne 'currentUser') {
    throw 'Installer compatibility identity changed; review the smoke contract first.'
}
$binaryName = 'mangamonitor-workbench-preview.exe'
$releaseDirectory = Join-Path $appDirectory 'src-tauri/target/x86_64-pc-windows-msvc/release'
$installerName = '{0}_{1}_x64-setup.exe' -f $config.productName, $config.version
$installerPath = Join-Path $releaseDirectory "bundle/nsis/$installerName"
$builtExecutable = Join-Path $releaseDirectory $binaryName
$temporaryRoot = [IO.Path]::GetFullPath($env:RUNNER_TEMP).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
$testDirectory = Join-Path $temporaryRoot ('mangamonitor-installer-' + [guid]::NewGuid().ToString('N'))
$installDirectory = Join-Path $testDirectory 'Installed Application'
if (-not [IO.Path]::GetFullPath($installDirectory).StartsWith($temporaryRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Installer destination must be inside RUNNER_TEMP.'
}
$installedExecutable = Join-Path $installDirectory $binaryName
$appDataDirectory = Join-Path ([Environment]::GetFolderPath('ApplicationData')) $config.identifier
$documents = Join-Path $appDataDirectory 'workbench-preview-v1'
$uninstallKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\' + $config.productName
$shortcutName = $config.productName + '.lnk'
$shortcuts = @(
    (Join-Path ([Environment]::GetFolderPath('Programs')) $shortcutName),
    (Join-Path ([Environment]::GetFolderPath('DesktopDirectory')) $shortcutName)
)
# Refuse existing app state instead of clearing or backing up anything on behalf
# of a user. The account slots are never written by this installer test.
if ((Test-Path -LiteralPath $appDataDirectory) -or (Test-Path -LiteralPath $uninstallKey) -or
    (Get-Process -Name 'mangamonitor-workbench-preview' -ErrorAction SilentlyContinue)) {
    throw 'The CI runner already contains this application or its data.'
}
foreach ($shortcut in $shortcuts) {
    if (Test-Path -LiteralPath $shortcut) { throw 'An application shortcut already exists on the CI runner.' }
}
if ($env:MANGAMONITOR_SMOKE_EXECUTABLE) { throw 'Unexpected inherited smoke executable.' }
$outputDirectory = Join-Path $appDirectory 'native-smoke-results'
$candidateDirectory = Join-Path $appDirectory 'release-candidate'
if (Test-Path -LiteralPath $candidateDirectory) { throw 'Release staging directory must be fresh.' }
[IO.Directory]::CreateDirectory($testDirectory) | Out-Null
[IO.Directory]::CreateDirectory($outputDirectory) | Out-Null
$evidence = [ordered]@{
    version = $config.version
    sourceRevision = $env:MANGAMONITOR_BUILD_REVISION
    checkoutRevision = $env:GITHUB_SHA
    workflowRun = $env:GITHUB_RUN_ID
    scope = 'fresh install, installed WebView startup/restart, same-version reinstall, uninstall preserving synthetic data, install again'
    historical034Upgrade = 'not tested'
    interactiveInstallerPages = 'not tested; silent current-user installation'
    binary = $null
    steps = [Collections.Generic.List[string]]::new()
    passed = $false
}
$resources = [ordered]@{
    'LICENSE.txt' = Join-Path $repository 'LICENSE'
    'THIRD_PARTY_NOTICES.md' = Join-Path $repository 'THIRD_PARTY_NOTICES.md'
    'USER_GUIDE.md' = Join-Path $repository 'docs/USER_GUIDE.md'
    'RELEASE_NOTES.md' = Join-Path $repository ('docs/RELEASE_NOTES_{0}.md' -f $config.version)
    'licenses/THIRD_PARTY_LICENSES.txt' = Join-Path $appDirectory 'src-tauri/release-resources/licenses/THIRD_PARTY_LICENSES.txt'
    'licenses/inventory.json' = Join-Path $appDirectory 'src-tauri/release-resources/licenses/inventory.json'
}
function Assert-SameFile([string] $Expected, [string] $Actual) {
    if (-not (Test-Path -LiteralPath $Actual -PathType Leaf) -or
        (Get-FileHash -LiteralPath $Expected -Algorithm SHA256).Hash -cne
        (Get-FileHash -LiteralPath $Actual -Algorithm SHA256).Hash) {
        throw ('Missing or changed installed artifact: ' + [IO.Path]::GetFileName($Actual))
    }
}
function Invoke-InstallerProcess([string] $Executable, [string] $Arguments) {
    $process = Start-Process -FilePath $Executable -ArgumentList $Arguments -WindowStyle Hidden -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw "Installer process failed with exit code $($process.ExitCode)." }
}
function Assert-Installed {
    if (-not (Test-Path -LiteralPath $installedExecutable -PathType Leaf)) {
        throw 'The installed executable is missing from the requested directory.'
    }
    if ($null -eq $evidence.binary) {
        $binaryEvidence = & node (Join-Path $repository 'scripts/verify-nsis-bundle.mjs') $builtExecutable $installedExecutable
        if ($LASTEXITCODE -ne 0) { throw 'Installed executable verification failed.' }
        $evidence.binary = $binaryEvidence | ConvertFrom-Json
    } elseif ((Get-FileHash -LiteralPath $installedExecutable -Algorithm SHA256).Hash -ine $evidence.binary.installedSha256) {
        throw 'Reinstalled executable differs from the verified first installation.'
    }
    foreach ($resource in $resources.GetEnumerator()) {
        Assert-SameFile $resource.Value (Join-Path $installDirectory $resource.Key)
    }
    $registration = Get-ItemProperty -LiteralPath $uninstallKey
    if ($registration.DisplayVersion -cne $config.version -or
        $registration.MainBinaryName -cne $binaryName -or
        $registration.InstallLocation.Trim('"') -ine $installDirectory) {
        throw 'Installed version, binary name or location did not match the release package.'
    }
    # The pinned Tauri NSIS template creates both links during silent installs
    # when /NS is absent and startMenuFolder is unset (as in our config).
    $shell = New-Object -ComObject WScript.Shell
    try {
        foreach ($shortcut in $shortcuts) {
            if (-not (Test-Path -LiteralPath $shortcut -PathType Leaf)) { throw 'Installed shortcut is missing.' }
            $link = $shell.CreateShortcut($shortcut)
            try {
                if ($link.TargetPath -ine $installedExecutable) { throw 'Installed shortcut targets another executable.' }
            } finally {
                [Runtime.InteropServices.Marshal]::ReleaseComObject($link) | Out-Null
            }
        }
    } finally {
        [Runtime.InteropServices.Marshal]::ReleaseComObject($shell) | Out-Null
    }
}
function Read-DocumentHashes {
    $hashes = [ordered]@{}
    foreach ($file in Get-ChildItem -LiteralPath $documents -File -Recurse | Sort-Object FullName) {
        if ($file.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Unexpected document link.' }
        $hashes[[IO.Path]::GetRelativePath($documents, $file.FullName)] = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash
    }
    if ($hashes.Count -eq 0) { throw 'No synthetic native documents were produced.' }
    return ($hashes | ConvertTo-Json -Compress)
}
function Assert-Retained {
    if ((Read-DocumentHashes) -cne $documentHashes -or
        (Get-FileHash -LiteralPath $externalProbe -Algorithm SHA256).Hash -cne $externalHash) {
        throw 'Installer changed synthetic application documents or the external-file probe.'
    }
}
try {
    # /D must be the last NSIS argument and must not be quoted, even with spaces.
    # See https://nsis.sourceforge.io/Docs/Chapter3.html#installerusage
    Invoke-InstallerProcess $installerPath "/S /D=$installDirectory"
    Assert-Installed
    $evidence.steps.Add('fresh-install-registration-shortcuts-and-bundled-resource-hashes')

    # The existing native suite runs exactly once, against the installed copy.
    $env:MANGAMONITOR_SMOKE_EXECUTABLE = $installedExecutable
    Push-Location $appDirectory
    try {
        & node tests/native-webview-smoke.mjs
        if ($LASTEXITCODE -ne 0) { throw 'Installed native WebView smoke failed.' }
    } finally {
        Pop-Location
        Remove-Item Env:MANGAMONITOR_SMOKE_EXECUTABLE
    }
    $evidence.steps.Add('installed-native-webview-startup-restart-and-reader-lifecycle')
    Set-Content -LiteralPath (Join-Path $documents 'installer-retention-probe.txt') -Value 'synthetic-ci-only' -Encoding utf8
    $externalProbe = Join-Path $testDirectory 'external-file-probe.txt'
    Set-Content -LiteralPath $externalProbe -Value 'synthetic-external-file; no real library was registered' -Encoding utf8
    $documentHashes = Read-DocumentHashes
    $externalHash = (Get-FileHash -LiteralPath $externalProbe -Algorithm SHA256).Hash

    Invoke-InstallerProcess $installerPath "/S /D=$installDirectory"
    Assert-Installed
    Assert-Retained
    $evidence.steps.Add('same-version-reinstall-preserved-synthetic-documents')

    # _?= keeps this process synchronous; the silent uninstall leaves the
    # Delete app data checkbox unselected. No recursive shell deletion is used.
    Invoke-InstallerProcess (Join-Path $installDirectory 'uninstall.exe') "/S _?=$installDirectory"
    if ((Test-Path -LiteralPath $installedExecutable) -or (Test-Path -LiteralPath $uninstallKey)) {
        throw 'Uninstall left the executable or its registration behind.'
    }
    foreach ($shortcut in $shortcuts) {
        if (Test-Path -LiteralPath $shortcut) { throw 'Uninstall left an application shortcut behind.' }
    }
    Assert-Retained
    $evidence.steps.Add('uninstall-removed-executable-registration-shortcuts-and-retained-documents')

    Invoke-InstallerProcess $installerPath "/S /D=$installDirectory"
    Assert-Installed
    Assert-Retained
    $evidence.steps.Add('install-after-uninstall-preserved-synthetic-documents')

    [IO.Directory]::CreateDirectory($candidateDirectory) | Out-Null
    Copy-Item -LiteralPath $installerPath -Destination (Join-Path $candidateDirectory $installerName)
    Copy-Item -LiteralPath $installedExecutable -Destination (Join-Path $candidateDirectory $binaryName)
    foreach ($resource in $resources.GetEnumerator()) {
        $destination = Join-Path $candidateDirectory $resource.Key
        [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($destination)) | Out-Null
        Copy-Item -LiteralPath (Join-Path $installDirectory $resource.Key) -Destination $destination
    }
    $artifacts = @(Get-ChildItem -LiteralPath $candidateDirectory -File -Recurse | Sort-Object FullName | ForEach-Object {
        [ordered]@{
            file = [IO.Path]::GetRelativePath($candidateDirectory, $_.FullName).Replace('\', '/')
            sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
            bytes = $_.Length
        }
    })
    $evidence.passed = $true
    $manifest = [ordered]@{
        schemaVersion = 1
        productName = $config.productName
        version = $config.version
        identifier = $config.identifier
        sourceRevision = $env:MANGAMONITOR_BUILD_REVISION
        checkoutRevision = $env:GITHUB_SHA
        ciRunUrl = "https://github.com/$($env:GITHUB_REPOSITORY)/actions/runs/$($env:GITHUB_RUN_ID)"
        signature = [ordered]@{
            installer = [string](Get-AuthenticodeSignature -LiteralPath $installerPath).Status
            executable = [string](Get-AuthenticodeSignature -LiteralPath $installedExecutable).Status
        }
        validation = $evidence
        artifacts = $artifacts
    }
    $manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $candidateDirectory 'manifest.json') -Encoding utf8
    Write-Output 'INSTALLER_SMOKE_PASSED: current release package only; historical 0.3.4 upgrade remains unverified.'
} finally {
    $evidence | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $outputDirectory 'installer-lifecycle.json') -Encoding utf8
}
