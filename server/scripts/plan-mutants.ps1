[CmdletBinding()]
param(
    [string]$RunId = ("manual-" + (Get-Date -Format "yyyyMMdd-HHmmss")),
    [ValidateRange(1, 64)]
    [int]$ShardCount = 8,
    [ValidateRange(1, 32)]
    [int]$Jobs = 2,
    [ValidateRange(30, 86400)]
    [int]$TimeoutSeconds = 180,
    [ValidateRange(30, 86400)]
    [int]$BuildTimeoutSeconds = 180,
    [string]$Package,
    [string]$TestPackage,
    [string]$File,
    [string]$OutputRoot
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$serverRoot = Split-Path -Parent $PSScriptRoot

function Normalize-RunId {
    param([string]$Value)

    $normalized = $Value.Trim()
    if ([string]::IsNullOrWhiteSpace($normalized)) {
        throw "RunId must not be empty."
    }

    $normalized = [regex]::Replace($normalized, '[^A-Za-z0-9._-]+', '-')
    $normalized = $normalized.Trim('-')
    if ([string]::IsNullOrWhiteSpace($normalized)) {
        throw "RunId must contain at least one alphanumeric character."
    }

    return $normalized
}

function Resolve-CampaignBaseRoot {
    param([string]$Candidate)

    if ([string]::IsNullOrWhiteSpace($Candidate)) {
        return Join-Path $serverRoot "target\reports\mutants-campaigns"
    }

    if ([System.IO.Path]::IsPathRooted($Candidate)) {
        return [System.IO.Path]::GetFullPath($Candidate)
    }

    return [System.IO.Path]::GetFullPath((Join-Path $serverRoot $Candidate))
}

function Quote-PowerShellLiteral {
    param([string]$Value)

    return "'" + $Value.Replace("'", "''") + "'"
}

$normalizedRunId = Normalize-RunId -Value $RunId
$campaignBaseRoot = Resolve-CampaignBaseRoot -Candidate $OutputRoot
$campaignRoot = Join-Path $campaignBaseRoot $normalizedRunId

New-Item -ItemType Directory -Force -Path $campaignRoot | Out-Null

$commands = @()
for ($index = 1; $index -le $ShardCount; $index += 1) {
    $shard = "{0}/{1}" -f $index, $ShardCount
    $parts = @(
        "pwsh -NoProfile -ExecutionPolicy Bypass -File ./scripts/run-mutants-shard.ps1"
        "-RunId", (Quote-PowerShellLiteral $normalizedRunId)
        "-Shard", (Quote-PowerShellLiteral $shard)
        "-Jobs", $Jobs
        "-TimeoutSeconds", $TimeoutSeconds
        "-BuildTimeoutSeconds", $BuildTimeoutSeconds
        "-OutputRoot", (Quote-PowerShellLiteral $campaignBaseRoot)
    )
    if (-not [string]::IsNullOrWhiteSpace($Package)) {
        $parts += @("-Package", (Quote-PowerShellLiteral $Package))
    }
    if (-not [string]::IsNullOrWhiteSpace($TestPackage)) {
        $parts += @("-TestPackage", (Quote-PowerShellLiteral $TestPackage))
    }
    if (-not [string]::IsNullOrWhiteSpace($File)) {
        $parts += @("-File", (Quote-PowerShellLiteral $File))
    }

    $commands += ($parts -join " ")
}

$summaryCommand = @(
    "pwsh -NoProfile -ExecutionPolicy Bypass -File ./scripts/summarize-mutants.ps1"
    "-RunId", (Quote-PowerShellLiteral $normalizedRunId)
    "-OutputRoot", (Quote-PowerShellLiteral $campaignBaseRoot)
) -join " "

$manifest = [ordered]@{
    run_id                = $normalizedRunId
    campaign_root         = $campaignRoot
    shard_count           = $ShardCount
    jobs                  = $Jobs
    timeout_seconds       = $TimeoutSeconds
    build_timeout_seconds = $BuildTimeoutSeconds
    package               = $Package
    test_package          = $TestPackage
    file                  = $File
    shard_commands        = $commands
    summary_command       = $summaryCommand
    created_utc           = (Get-Date).ToUniversalTime().ToString("o")
}
$manifest | ConvertTo-Json -Depth 5 | Set-Content -Path (Join-Path $campaignRoot "campaign.json") -Encoding utf8

$commandsText = @(
    "# Run each shard separately. Re-running the same shard will replace only that shard's output."
    $commands
    ""
    "# Rebuild the aggregate summary at any time."
    $summaryCommand
)
Set-Content -Path (Join-Path $campaignRoot "commands.ps1") -Value $commandsText -Encoding utf8

$planLines = @(
    "# Mutation Campaign"
    ""
    "- Run ID: $normalizedRunId"
    "- Campaign root: $campaignRoot"
    "- Shards: $ShardCount"
    "- Jobs per shard: $Jobs"
    "- Timeout: ${TimeoutSeconds}s"
    "- Build timeout: ${BuildTimeoutSeconds}s"
    "- Package filter: $(if ([string]::IsNullOrWhiteSpace($Package)) { "<workspace>" } else { $Package })"
    "- Test package filter: $(if ([string]::IsNullOrWhiteSpace($TestPackage)) { "<default>" } else { $TestPackage })"
    "- File filter: $(if ([string]::IsNullOrWhiteSpace($File)) { "<all configured files>" } else { $File })"
    ""
    "## Run"
    ""
    '```powershell'
    $commands
    '```'
    ""
    "## Summarize"
    ""
    '```powershell'
    $summaryCommand
    '```'
)
Set-Content -Path (Join-Path $campaignRoot "plan.md") -Value $planLines -Encoding utf8

Write-Host ("[mutants-plan] run_id={0}" -f $normalizedRunId)
Write-Host ("[mutants-plan] campaign_root={0}" -f $campaignRoot)
Write-Host ("[mutants-plan] commands_file={0}" -f (Join-Path $campaignRoot "commands.ps1"))
Write-Host ("[mutants-plan] summary_command={0}" -f $summaryCommand)

foreach ($command in $commands) {
    Write-Host $command
}
