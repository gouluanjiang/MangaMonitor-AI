# Configure only this disposable CI application's supported WebView2 overrides.
# Elevated WebView2 hosts ignore environment overrides since Runtime 150:
# https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/security
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('Configure', 'Restore')]
    [string] $Mode,
    [Parameter(Mandatory = $true)]
    [ValidateRange(1024, 65535)]
    [int] $DebuggingPort,
    [Parameter(Mandatory = $true)]
    [string] $ProfileDirectory
)

$ErrorActionPreference = 'Stop'
if ($env:CI -cne 'true' -or $env:GITHUB_ACTIONS -cne 'true' -or -not $IsWindows) {
    throw 'WebView test configuration is restricted to disposable Windows GitHub CI.'
}
$profilePath = [IO.Path]::GetFullPath($ProfileDirectory)
$temporaryRoot = [IO.Path]::GetFullPath($env:RUNNER_TEMP).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
if (-not $profilePath.StartsWith($temporaryRoot, [StringComparison]::OrdinalIgnoreCase) -or
    -not [IO.Path]::GetFileName($profilePath).StartsWith('mangamonitor-webview-', [StringComparison]::Ordinal)) {
    throw 'The WebView profile must be the generated directory inside RUNNER_TEMP.'
}
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
$elevated = $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
$identity.Dispose()
if (-not $elevated) {
    Write-Output 'CI_WEBVIEW_CONFIG: standard integrity; environment overrides apply.'
    exit 0
}

$application = 'mangamonitor-workbench-preview.exe'
$settings = @(
    @{ Path = 'HKLM:\SOFTWARE\Policies\Microsoft\Edge\WebView2\AdditionalBrowserArguments'; Value = "--remote-debugging-port=$DebuggingPort" },
    @{ Path = 'HKLM:\SOFTWARE\Policies\Microsoft\Edge\WebView2\UserDataFolder'; Value = $profilePath }
)
function Read-AppValue($Setting) {
    Get-ItemPropertyValue -LiteralPath $Setting.Path -Name $application -ErrorAction SilentlyContinue
}
if ($Mode -eq 'Restore') {
    foreach ($setting in $settings) {
        $current = Read-AppValue $setting
        if ($null -eq $current) { continue }
        if ($current -cne $setting.Value) { throw 'The CI application policy changed externally; refusing to remove it.' }
        Remove-ItemProperty -LiteralPath $setting.Path -Name $application
    }
    Write-Output 'CI_WEBVIEW_CONFIG: owned application overrides removed.'
    exit 0
}

# Refuse to replace any pre-existing setting; never change a wildcard or another
# application's policy. The runner itself is discarded after this job.
foreach ($setting in $settings) {
    if ($null -ne (Read-AppValue $setting)) { throw 'Pre-existing application policy must not be replaced by the smoke test.' }
}
$created = [Collections.Generic.List[object]]::new()
try {
    foreach ($setting in $settings) {
        if (-not (Test-Path -LiteralPath $setting.Path)) {
            New-Item -Path $setting.Path -Force | Out-Null
        }
        New-ItemProperty -LiteralPath $setting.Path -Name $application -Value $setting.Value -PropertyType String | Out-Null
        $created.Add($setting)
    }
} catch {
    foreach ($setting in $created) {
        if ((Read-AppValue $setting) -ceq $setting.Value) {
            Remove-ItemProperty -LiteralPath $setting.Path -Name $application
        }
    }
    throw
}
Write-Output 'CI_WEBVIEW_CONFIG: elevated host; installed supported per-application HKLM overrides.'
