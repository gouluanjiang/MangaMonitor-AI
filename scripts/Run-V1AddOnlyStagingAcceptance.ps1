[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$StateDir,

    [Parameter(Mandatory = $true)]
    [string]$CommandFile,

    [Parameter(Mandatory = $true)]
    [string]$StagingRoot,

    [string]$Gates,

    [string]$ReportPath
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Fail([string]$Code) {
    throw $Code
}

function Resolve-ExistingDirectory([string]$Path, [string]$Code) {
    if (-not (Test-Path -LiteralPath $Path -PathType Container)) {
        Fail $Code
    }
    return (Resolve-Path -LiteralPath $Path).Path
}

function Resolve-ExistingFile([string]$Path, [string]$Code) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        Fail $Code
    }
    return (Resolve-Path -LiteralPath $Path).Path
}

function Is-SameOrUnder([string]$Candidate, [string]$Root) {
    $candidateFull = [System.IO.Path]::GetFullPath($Candidate).TrimEnd('\', '/')
    $rootFull = [System.IO.Path]::GetFullPath($Root).TrimEnd('\', '/')
    if ($candidateFull.Equals($rootFull, [System.StringComparison]::OrdinalIgnoreCase)) {
        return $true
    }
    $rootPrefix = $rootFull + [System.IO.Path]::DirectorySeparatorChar
    return $candidateFull.StartsWith($rootPrefix, [System.StringComparison]::OrdinalIgnoreCase)
}

function Snapshot-Tree([string]$Root) {
    $rootFull = [System.IO.Path]::GetFullPath($Root).TrimEnd('\', '/')
    $snapshot = [ordered]@{}
    Get-ChildItem -LiteralPath $rootFull -Directory -Recurse -Force | Sort-Object FullName | ForEach-Object {
        $relative = $_.FullName.Substring($rootFull.Length).TrimStart('\', '/').Replace('\', '/')
        $snapshot[$relative] = 'D'
    }
    Get-ChildItem -LiteralPath $rootFull -File -Recurse -Force | Sort-Object FullName | ForEach-Object {
        $relative = $_.FullName.Substring($rootFull.Length).TrimStart('\', '/').Replace('\', '/')
        $digest = (Get-FileHash -Algorithm SHA256 -LiteralPath $_.FullName).Hash.ToLowerInvariant()
        $snapshot[$relative] = "F:$digest"
    }
    return $snapshot
}

function Assert-SnapshotEqual($Before, $After, [string]$Code) {
    $beforeJson = $Before | ConvertTo-Json -Compress -Depth 20
    $afterJson = $After | ConvertTo-Json -Compress -Depth 20
    if ($beforeJson -cne $afterJson) {
        Fail $Code
    }
}

function Invoke-CargoJson([string[]]$Arguments, [string]$FailureCode) {
    $lines = & cargo @Arguments 2>&1
    if ($LASTEXITCODE -ne 0) {
        $lines | ForEach-Object { Write-Host $_ }
        Fail $FailureCode
    }
    $text = ($lines | Out-String).Trim()
    try {
        return $text | ConvertFrom-Json
    } catch {
        $lines | ForEach-Object { Write-Host $_ }
        Fail "${FailureCode}_INVALID_JSON"
    }
}

if ($env:GITHUB_ACTIONS -eq 'true') {
    Fail 'V1_ADD_ONLY_ACCEPTANCE_GITHUB_ACTIONS_FORBIDDEN'
}

$stateFull = Resolve-ExistingDirectory $StateDir 'V1_ADD_ONLY_ACCEPTANCE_STATE_DIR_MISSING'
$commandFull = Resolve-ExistingFile $CommandFile 'V1_ADD_ONLY_ACCEPTANCE_COMMAND_MISSING'
$stagingFull = Resolve-ExistingDirectory $StagingRoot 'V1_ADD_ONLY_ACCEPTANCE_STAGING_ROOT_MISSING'
$commandsRoot = Join-Path $stagingFull 'commands'
if (-not (Test-Path -LiteralPath $commandsRoot -PathType Container)) {
    Fail 'V1_ADD_ONLY_ACCEPTANCE_COMMANDS_DIR_MISSING'
}
$commandsFull = (Resolve-Path -LiteralPath $commandsRoot).Path

if ((Is-SameOrUnder $stateFull $stagingFull) -or (Is-SameOrUnder $stagingFull $stateFull)) {
    Fail 'V1_ADD_ONLY_ACCEPTANCE_STATE_STAGING_MUST_BE_SEPARATE'
}

$gateFull = $null
if ($Gates) {
    $gateFull = Resolve-ExistingFile $Gates 'V1_ADD_ONLY_ACCEPTANCE_GATES_MISSING'
}

$reportFull = $null
if ($ReportPath) {
    $reportFull = [System.IO.Path]::GetFullPath($ReportPath)
    if ((Is-SameOrUnder $reportFull $stateFull) -or (Is-SameOrUnder $reportFull $stagingFull)) {
        Fail 'V1_ADD_ONLY_ACCEPTANCE_REPORT_MUST_BE_OUTSIDE_STATE_AND_STAGING'
    }
}

try {
    $command = Get-Content -LiteralPath $commandFull -Raw | ConvertFrom-Json
} catch {
    Fail 'V1_ADD_ONLY_ACCEPTANCE_COMMAND_INVALID_JSON'
}

if ($command.action -cne 'download') {
    Fail 'V1_ADD_ONLY_ACCEPTANCE_REQUIRES_DOWNLOAD_ACTION'
}
if ($command.intent -cne 'DOWNLOAD_TO_STAGING_ONLY') {
    Fail 'V1_ADD_ONLY_ACCEPTANCE_REQUIRES_STAGING_ONLY_INTENT'
}
if (-not $command.command_id -or -not $command.task_id -or -not $command.work_id) {
    Fail 'V1_ADD_ONLY_ACCEPTANCE_COMMAND_BINDING_MISSING'
}
if ($command.source -cne 'jm' -and $command.source -cne 'pica') {
    Fail 'V1_ADD_ONLY_ACCEPTANCE_SOURCE_UNSUPPORTED'
}
if ($command.source -ceq 'pica' -and [string]::IsNullOrWhiteSpace($env:PICA_TOKEN)) {
    Fail 'V1_ADD_ONLY_ACCEPTANCE_PICA_TOKEN_REQUIRED'
}

$commandDir = Join-Path $commandsFull ([string]$command.command_id)
if (Test-Path -LiteralPath $commandDir) {
    Fail 'V1_ADD_ONLY_ACCEPTANCE_COMMAND_DIR_ALREADY_EXISTS'
}

$queueArgs = @('run', '--quiet', '--locked', '-p', 'cloud-monitor', '--bin', 'assistant-executor-queue', '--', '--state', $stateFull, '--offset', '0', '--limit', '200')
if ($gateFull) {
    $queueArgs += @('--gates', $gateFull)
}
$queue = Invoke-CargoJson $queueArgs 'V1_ADD_ONLY_ACCEPTANCE_QUEUE_FAILED'
$matching = @($queue.commands | Where-Object { $_.command_id -ceq $command.command_id })
if ($matching.Count -ne 1) {
    Fail 'V1_ADD_ONLY_ACCEPTANCE_COMMAND_NOT_IN_CURRENT_QUEUE'
}
if ($matching[0].action -cne 'download' -or $matching[0].intent -cne 'DOWNLOAD_TO_STAGING_ONLY') {
    Fail 'V1_ADD_ONLY_ACCEPTANCE_QUEUE_SCOPE_MISMATCH'
}

$stateBefore = Snapshot-Tree $stateFull
$stagingBefore = Snapshot-Tree $stagingFull
$commandHashBefore = (Get-FileHash -Algorithm SHA256 -LiteralPath $commandFull).Hash.ToLowerInvariant()

$executorArgs = @('run', '--quiet', '--locked', '-p', 'cloud-monitor', '--bin', 'mangamonitor-local-executor', '--', '--state', $stateFull, '--command', $commandFull, '--staging-root', $stagingFull)
if ($gateFull) {
    $executorArgs += @('--gates', $gateFull)
}
$report = Invoke-CargoJson $executorArgs 'V1_ADD_ONLY_ACCEPTANCE_EXECUTOR_FAILED'

$commandHashAfter = (Get-FileHash -Algorithm SHA256 -LiteralPath $commandFull).Hash.ToLowerInvariant()
if ($commandHashBefore -cne $commandHashAfter) {
    Fail 'V1_ADD_ONLY_ACCEPTANCE_COMMAND_FILE_CHANGED'
}

$stateAfter = Snapshot-Tree $stateFull
Assert-SnapshotEqual $stateBefore $stateAfter 'V1_ADD_ONLY_ACCEPTANCE_STATE_MUTATED'

if (-not (Test-Path -LiteralPath $commandDir -PathType Container)) {
    Fail 'V1_ADD_ONLY_ACCEPTANCE_COMMAND_DIR_MISSING_AFTER_EXECUTION'
}
$resolvedCommandDir = (Resolve-Path -LiteralPath $commandDir).Path
$expectedCommandDir = [System.IO.Path]::GetFullPath($commandDir)
if (-not $resolvedCommandDir.Equals($expectedCommandDir, [System.StringComparison]::OrdinalIgnoreCase)) {
    Fail 'V1_ADD_ONLY_ACCEPTANCE_COMMAND_DIR_RESOLUTION_DRIFT'
}

$stagingAfter = Snapshot-Tree $stagingFull
$allowedRoot = 'commands/' + [string]$command.command_id
$allowedPrefix = $allowedRoot + '/'
$allKeys = @($stagingBefore.Keys + $stagingAfter.Keys | Sort-Object -Unique)
foreach ($key in $allKeys) {
    $beforeValue = if ($stagingBefore.Contains($key)) { $stagingBefore[$key] } else { $null }
    $afterValue = if ($stagingAfter.Contains($key)) { $stagingAfter[$key] } else { $null }
    $insideCommand = ([string]$key -ceq $allowedRoot) -or ([string]$key).StartsWith($allowedPrefix, [System.StringComparison]::Ordinal)
    if ($beforeValue -cne $afterValue -and -not $insideCommand) {
        Fail 'V1_ADD_ONLY_ACCEPTANCE_STAGING_ESCAPED_COMMAND_DIR'
    }
}

if ($report.command_id -cne $command.command_id -or $report.task_id -cne $command.task_id -or $report.work_id -cne $command.work_id) {
    Fail 'V1_ADD_ONLY_ACCEPTANCE_REPORT_BINDING_MISMATCH'
}
if ($report.staging_execution_completed -ne $true -or $report.receipt.outcome -cne 'SUCCEEDED') {
    Fail 'V1_ADD_ONLY_ACCEPTANCE_EXECUTION_NOT_COMPLETE'
}
if ($report.receipt_view.ready_for_inventory_verification -ne $true) {
    Fail 'V1_ADD_ONLY_ACCEPTANCE_RECEIPT_NOT_CURRENT'
}
foreach ($property in @('inventory_mutation_authorized', 'task_completion_authorized', 'promotion_authorized', 'replacement_authorized', 'physical_delete_authorized', 'production_enablement_authorized')) {
    if ($report.$property -ne $false) {
        Fail "V1_ADD_ONLY_ACCEPTANCE_UNSAFE_AUTHORITY_$property"
    }
}
if ($report.source_completion.inventory_mutation_authorized -ne $false -or
    $report.source_completion.task_completion_authorized -ne $false -or
    $report.source_completion.promotion_authorized -ne $false -or
    $report.source_completion.replacement_authorized -ne $false -or
    $report.source_completion.physical_delete_authorized -ne $false) {
    Fail 'V1_ADD_ONLY_ACCEPTANCE_SOURCE_PROOF_ESCALATED_AUTHORITY'
}

$commandFiles = @(Get-ChildItem -LiteralPath $resolvedCommandDir -File -Recurse -Force)
if ($commandFiles.Count -eq 0) {
    Fail 'V1_ADD_ONLY_ACCEPTANCE_NO_STAGED_FILES'
}

$acceptance = [ordered]@{
    schema_version = 1
    status = 'V1_ADD_ONLY_STAGING_ACCEPTED'
    command_id = [string]$command.command_id
    task_id = [string]$command.task_id
    work_id = [string]$command.work_id
    source = [string]$command.source
    staging_root = $stagingFull
    command_dir = $resolvedCommandDir
    staged_file_count = $commandFiles.Count
    monitor_state_unchanged = $true
    staging_changes_confined_to_command_dir = $true
    ready_for_inventory_verification = $true
    inventory_mutation_authorized = $false
    task_completion_authorized = $false
    promotion_authorized = $false
    replacement_authorized = $false
    physical_delete_authorized = $false
    production_enablement_authorized = $false
    execution_report = $report
}

$json = $acceptance | ConvertTo-Json -Depth 100
if ($reportFull) {
    $reportParent = Split-Path -Parent $reportFull
    if ($reportParent -and -not (Test-Path -LiteralPath $reportParent -PathType Container)) {
        New-Item -ItemType Directory -Path $reportParent -Force | Out-Null
    }
    [System.IO.File]::WriteAllText($reportFull, $json, [System.Text.UTF8Encoding]::new($false))
}
$json
