# Interactive: run this script yourself in PowerShell. Nothing secret is printed or saved to disk.
param([switch]$SetGitHubSecrets)
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\Use-LocalTools.ps1"
$phaseRoot = Split-Path -Parent $PSScriptRoot
$env:PICA_EMAIL = Read-Host 'Pica email (only entered locally)'
$securePassword = Read-Host 'Pica password (hidden)' -AsSecureString
$ptr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($securePassword)
try {
    $env:PICA_PASSWORD = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($ptr)
    Push-Location $phaseRoot
    & "$phaseRoot\target\debug\cloud-monitor.exe" --source pica --report reports/local-pica-login.json
    if ($LASTEXITCODE -ne 0) { throw 'Pica login smoke failed. See sanitized report.' }
    if ($SetGitHubSecrets) {
        $env:PICA_EMAIL | gh secret set PICA_EMAIL --repo gouluanjiang/MangaMonitor
        if ($LASTEXITCODE -ne 0) { throw 'Could not set PICA_EMAIL secret' }
        $env:PICA_PASSWORD | gh secret set PICA_PASSWORD --repo gouluanjiang/MangaMonitor
        if ($LASTEXITCODE -ne 0) { throw 'Could not set PICA_PASSWORD secret' }
        gh workflow run phase1a.yml --repo gouluanjiang/MangaMonitor -f source=pica
    }
} finally {
    [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($ptr)
    Remove-Item Env:PICA_EMAIL -ErrorAction SilentlyContinue
    Remove-Item Env:PICA_PASSWORD -ErrorAction SilentlyContinue
    Pop-Location
}
