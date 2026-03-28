[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$RunId,
    [Parameter(Mandatory = $true)]
    [string]$Shard,
    [ValidateRange(1, 32)]
    [int]$Jobs = 2,
    [ValidateRange(30, 86400)]
    [int]$TimeoutSeconds = 180,
    [ValidateRange(30, 86400)]
    [int]$BuildTimeoutSeconds = 180,
    [string]$Package,
    [string]$TestPackage,
    [string]$File,
    [string]$OutputRoot,
    [switch]$NoSummary
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

function Normalize-ShardLabel {
    param([string]$Value)

    $trimmed = $Value.Trim()
    if ($trimmed -notmatch '^\d+/\d+$') {
        throw "Shard must be in the form N/M, for example 0/8."
    }

    return $trimmed.Replace('/', '-of-')
}

function Assert-ValidShardDescriptor {
    param([string]$Value)

    $trimmed = $Value.Trim()
    if ($trimmed -notmatch '^(?<index>\d+)/(?<count>\d+)$') {
        throw "Shard must be in the form N/M, for example 0/8."
    }

    $index = [int]$Matches["index"]
    $count = [int]$Matches["count"]
    if ($count -le 0) {
        throw "Shard count must be greater than zero."
    }
    if ($index -lt 0 -or $index -ge $count) {
        throw ("Shard index must be zero-based and strictly less than the shard count; got {0}/{1}." -f $index, $count)
    }
}

$normalizedRunId = Normalize-RunId -Value $RunId
$campaignBaseRoot = Resolve-CampaignBaseRoot -Candidate $OutputRoot
$campaignRoot = Join-Path $campaignBaseRoot $normalizedRunId
Assert-ValidShardDescriptor -Value $Shard
$shardLabel = Normalize-ShardLabel -Value $Shard
$shardRoot = Join-Path (Join-Path $campaignRoot "shards") $shardLabel

New-Item -ItemType Directory -Force -Path $shardRoot | Out-Null

$runMetadata = [ordered]@{
    run_id                = $normalizedRunId
    shard                 = $Shard
    output_directory      = $shardRoot
    jobs                  = $Jobs
    timeout_seconds       = $TimeoutSeconds
    build_timeout_seconds = $BuildTimeoutSeconds
    package               = $Package
    test_package          = $TestPackage
    file                  = $File
    started_utc           = (Get-Date).ToUniversalTime().ToString("o")
}
$runMetadata | ConvertTo-Json -Depth 4 | Set-Content -Path (Join-Path $shardRoot "run.json") -Encoding utf8

$previousEnv = @{
    RARENA_MUTANTS_OUTPUT_DIR   = $env:RARENA_MUTANTS_OUTPUT_DIR
    RARENA_MUTANTS_SHARD        = $env:RARENA_MUTANTS_SHARD
    RARENA_MUTANTS_JOBS         = $env:RARENA_MUTANTS_JOBS
    RARENA_MUTANTS_TIMEOUT      = $env:RARENA_MUTANTS_TIMEOUT
    RARENA_MUTANTS_BUILD_TIMEOUT = $env:RARENA_MUTANTS_BUILD_TIMEOUT
    RARENA_MUTANTS_PACKAGE      = $env:RARENA_MUTANTS_PACKAGE
    RARENA_MUTANTS_TEST_PACKAGE = $env:RARENA_MUTANTS_TEST_PACKAGE
    RARENA_MUTANTS_FILE         = $env:RARENA_MUTANTS_FILE
}

$caughtError = $null

try {
    $env:RARENA_MUTANTS_OUTPUT_DIR = $shardRoot
    $env:RARENA_MUTANTS_SHARD = $Shard
    $env:RARENA_MUTANTS_JOBS = [string]$Jobs
    $env:RARENA_MUTANTS_TIMEOUT = [string]$TimeoutSeconds
    $env:RARENA_MUTANTS_BUILD_TIMEOUT = [string]$BuildTimeoutSeconds

    if ([string]::IsNullOrWhiteSpace($Package)) {
        Remove-Item Env:RARENA_MUTANTS_PACKAGE -ErrorAction SilentlyContinue
    }
    else {
        $env:RARENA_MUTANTS_PACKAGE = $Package
    }

    if ([string]::IsNullOrWhiteSpace($TestPackage)) {
        Remove-Item Env:RARENA_MUTANTS_TEST_PACKAGE -ErrorAction SilentlyContinue
    }
    else {
        $env:RARENA_MUTANTS_TEST_PACKAGE = $TestPackage
    }

    if ([string]::IsNullOrWhiteSpace($File)) {
        Remove-Item Env:RARENA_MUTANTS_FILE -ErrorAction SilentlyContinue
    }
    else {
        $env:RARENA_MUTANTS_FILE = $File
    }

    Write-Host ("[mutants-run] run_id={0}" -f $normalizedRunId)
    Write-Host ("[mutants-run] shard={0}" -f $Shard)
    Write-Host ("[mutants-run] output={0}" -f $shardRoot)

    & (Join-Path $PSScriptRoot "quality.ps1") mutants
}
catch {
    $caughtError = $_
}
finally {
    foreach ($entry in $previousEnv.GetEnumerator()) {
        if ($null -eq $entry.Value) {
            Remove-Item ("Env:{0}" -f $entry.Key) -ErrorAction SilentlyContinue
        }
        else {
            Set-Item ("Env:{0}" -f $entry.Key) -Value $entry.Value
        }
    }

    if (-not $NoSummary) {
        & (Join-Path $PSScriptRoot "summarize-mutants.ps1") -RunId $normalizedRunId -OutputRoot $campaignBaseRoot
    }
}

if ($null -ne $caughtError) {
    throw $caughtError
}

Write-Host ("[mutants-run] shard complete: {0}" -f $shardRoot)
if (-not $NoSummary) {
    Write-Host ("[mutants-run] updated summary: {0}" -f (Join-Path $campaignRoot "summary.md"))
}
