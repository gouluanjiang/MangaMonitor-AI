param([ValidateSet('both','jm','pica')][string]$Source = 'both', [string]$Report = 'reports/local-smoke.json')
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\Use-LocalTools.ps1"
$phaseRoot = Split-Path -Parent $PSScriptRoot
$configPath = Join-Path $env:APPDATA 'com.lanyeeee.picacomic-downloader\config.json'
if (($Source -eq 'both' -or $Source -eq 'pica') -and -not $env:PICA_TOKEN -and (Test-Path -LiteralPath $configPath)) {
    $config = Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json
    $env:PICA_TOKEN = $config.token
}
try {
    Push-Location $phaseRoot
    & "$phaseRoot\target\debug\cloud-monitor.exe" --source $Source --report $Report
    $smokeExit = $LASTEXITCODE
} finally {
    Pop-Location
    Remove-Item Env:PICA_TOKEN -ErrorAction SilentlyContinue
    Remove-Item Env:PICA_EMAIL -ErrorAction SilentlyContinue
    Remove-Item Env:PICA_PASSWORD -ErrorAction SilentlyContinue
}
exit $smokeExit
