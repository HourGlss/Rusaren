[CmdletBinding(DefaultParameterSetName = "RunId")]
param(
    [Parameter(Mandatory = $true, ParameterSetName = "RunId")]
    [string]$RunId,
    [Parameter(Mandatory = $true, ParameterSetName = "CampaignRoot")]
    [string]$CampaignRoot,
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

function Read-OutcomeCount {
    param(
        [string]$MutantsOutRoot,
        [string]$Name
    )

    $path = Join-Path $MutantsOutRoot ("{0}.txt" -f $Name)
    if (-not (Test-Path $path)) {
        return 0
    }

    return @(
        Get-Content -Path $path -ErrorAction SilentlyContinue |
            Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
    ).Count
}

function Read-OutcomeLines {
    param(
        [string]$MutantsOutRoot,
        [string]$Name
    )

    $path = Join-Path $MutantsOutRoot ("{0}.txt" -f $Name)
    if (-not (Test-Path $path)) {
        return @()
    }

    return @(
        Get-Content -Path $path -ErrorAction SilentlyContinue |
            Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
            ForEach-Object { $_.Trim() }
    )
}

function Read-TotalMutants {
    param([string]$MutantsOutRoot)

    $mutantsJsonPath = Join-Path $MutantsOutRoot "mutants.json"
    if (Test-Path $mutantsJsonPath) {
        try {
            $mutantsDoc = Get-Content -Path $mutantsJsonPath -Raw -ErrorAction SilentlyContinue | ConvertFrom-Json
            if ($null -ne $mutantsDoc) {
                return @($mutantsDoc).Count
            }
        }
        catch {
        }
    }

    $outcomesPath = Join-Path $MutantsOutRoot "outcomes.json"
    if (Test-Path $outcomesPath) {
        try {
            $outcomesDoc = Get-Content -Path $outcomesPath -Raw -ErrorAction SilentlyContinue | ConvertFrom-Json
            if ($null -ne $outcomesDoc -and $null -ne $outcomesDoc.total_mutants) {
                return [int]$outcomesDoc.total_mutants
            }
            if ($null -ne $outcomesDoc -and $null -ne $outcomesDoc.outcomes) {
                return @(
                    $outcomesDoc.outcomes |
                        Where-Object {
                            $scenario = $_.scenario
                            if ($scenario -is [string]) {
                                return $scenario -ne "Baseline"
                            }

                            return $true
                        }
                ).Count
            }
        }
        catch {
        }
    }

    return $null
}

function Read-RunDuration {
    param([string]$LogPath)

    if (-not (Test-Path $LogPath)) {
        return $null
    }

    $text = Get-Content -Path $LogPath -Raw -ErrorAction SilentlyContinue
    if ([string]::IsNullOrWhiteSpace($text)) {
        return $null
    }

    $match = [regex]::Match($text, '(?m)^\s*\d+\s+mutants tested in\s+(?<duration>[^:]+):')
    if ($match.Success) {
        return $match.Groups["duration"].Value.Trim()
    }

    return $null
}

function Get-MissedFileKey {
    param([string]$Mutant)

    $match = [regex]::Match($Mutant, '^(?<file>.+?):\d+:\d+:')
    if ($match.Success) {
        return $match.Groups["file"].Value
    }

    return "<unknown>"
}

function Get-ShardDescriptor {
    param([System.IO.DirectoryInfo]$ShardDirectory)

    $runJsonPath = Join-Path $ShardDirectory.FullName "run.json"
    if (Test-Path $runJsonPath) {
        try {
            $runDoc = Get-Content -Path $runJsonPath -Raw -ErrorAction SilentlyContinue | ConvertFrom-Json
            if ($null -ne $runDoc -and -not [string]::IsNullOrWhiteSpace([string]$runDoc.shard)) {
                return [string]$runDoc.shard
            }
        }
        catch {
        }
    }

    if ($ShardDirectory.Name -match '^(?<index>\d+)-of-(?<count>\d+)$') {
        return ("{0}/{1}" -f $Matches["index"], $Matches["count"])
    }

    return $null
}

function Test-ValidShardDescriptor {
    param([string]$Value)

    if ([string]::IsNullOrWhiteSpace($Value)) {
        return $false
    }
    if ($Value -notmatch '^(?<index>\d+)/(?<count>\d+)$') {
        return $false
    }

    $index = [int]$Matches["index"]
    $count = [int]$Matches["count"]
    return $count -gt 0 -and $index -ge 0 -and $index -lt $count
}

if ($PSCmdlet.ParameterSetName -eq "RunId") {
    $normalizedRunId = Normalize-RunId -Value $RunId
    $campaignBaseRoot = Resolve-CampaignBaseRoot -Candidate $OutputRoot
    $resolvedCampaignRoot = Join-Path $campaignBaseRoot $normalizedRunId
}
else {
    if ([System.IO.Path]::IsPathRooted($CampaignRoot)) {
        $resolvedCampaignRoot = [System.IO.Path]::GetFullPath($CampaignRoot)
    }
    else {
        $resolvedCampaignRoot = [System.IO.Path]::GetFullPath((Join-Path $serverRoot $CampaignRoot))
    }
    $normalizedRunId = Split-Path -Leaf $resolvedCampaignRoot
}

$shardsRoot = Join-Path $resolvedCampaignRoot "shards"
if (-not (Test-Path $shardsRoot)) {
    throw "No shard output directory exists at $shardsRoot."
}

$shardDirectories = @(
    Get-ChildItem -Path $shardsRoot -Directory -ErrorAction Stop |
        Sort-Object Name
)
if ($shardDirectories.Count -eq 0) {
    throw "No shard output directories were found under $shardsRoot."
}

$aggregate = [ordered]@{
    run_id        = $normalizedRunId
    campaign_root = $resolvedCampaignRoot
    generated_utc = (Get-Date).ToUniversalTime().ToString("o")
    shard_count   = $shardDirectories.Count
    totals        = [ordered]@{
        total     = 0
        caught    = 0
        missed    = 0
        timeout   = 0
        unviable  = 0
        completed = 0
    }
    shards        = @()
}

$allMissed = New-Object System.Collections.Generic.List[object]
$allTimeout = New-Object System.Collections.Generic.List[object]
$allUnviable = New-Object System.Collections.Generic.List[object]

foreach ($shardDirectory in $shardDirectories) {
    $mutantsOutRoot = Join-Path $shardDirectory.FullName "mutants.out"
    $logPath = Join-Path $shardDirectory.FullName "mutants.log"
    $caughtCount = Read-OutcomeCount -MutantsOutRoot $mutantsOutRoot -Name "caught"
    $missedCount = Read-OutcomeCount -MutantsOutRoot $mutantsOutRoot -Name "missed"
    $timeoutCount = Read-OutcomeCount -MutantsOutRoot $mutantsOutRoot -Name "timeout"
    $unviableCount = Read-OutcomeCount -MutantsOutRoot $mutantsOutRoot -Name "unviable"
    $totalCount = Read-TotalMutants -MutantsOutRoot $mutantsOutRoot
    $completedCount = $caughtCount + $missedCount + $timeoutCount + $unviableCount
    $shardDescriptor = Get-ShardDescriptor -ShardDirectory $shardDirectory
    if (-not (Test-ValidShardDescriptor -Value $shardDescriptor)) {
        continue
    }
    $status = if ($null -ne $totalCount -and $completedCount -eq $totalCount -and $totalCount -gt 0) {
        "completed"
    }
    elseif ($completedCount -gt 0) {
        "partial"
    }
    else {
        "not-started"
    }

    $missedLines = Read-OutcomeLines -MutantsOutRoot $mutantsOutRoot -Name "missed"
    foreach ($line in $missedLines) {
        $allMissed.Add([pscustomobject]@{
                shard  = $shardDirectory.Name
                mutant = $line
            })
    }

    $timeoutLines = Read-OutcomeLines -MutantsOutRoot $mutantsOutRoot -Name "timeout"
    foreach ($line in $timeoutLines) {
        $allTimeout.Add([pscustomobject]@{
                shard  = $shardDirectory.Name
                mutant = $line
            })
    }

    $unviableLines = Read-OutcomeLines -MutantsOutRoot $mutantsOutRoot -Name "unviable"
    foreach ($line in $unviableLines) {
        $allUnviable.Add([pscustomobject]@{
                shard  = $shardDirectory.Name
                mutant = $line
            })
    }

    $aggregate.totals.total += $(if ($null -eq $totalCount) { 0 } else { $totalCount })
    $aggregate.totals.caught += $caughtCount
    $aggregate.totals.missed += $missedCount
    $aggregate.totals.timeout += $timeoutCount
    $aggregate.totals.unviable += $unviableCount
    $aggregate.totals.completed += $completedCount

    $aggregate.shards += [pscustomobject]@{
        shard      = $shardDirectory.Name
        status     = $status
        total      = $totalCount
        caught     = $caughtCount
        missed     = $missedCount
        timeout    = $timeoutCount
        unviable   = $unviableCount
        completed  = $completedCount
        duration   = Read-RunDuration -LogPath $logPath
        directory  = $shardDirectory.FullName
        log_path   = $logPath
    }
}

$aggregate.shard_count = $aggregate.shards.Count

$topMissedFiles = @(
    $allMissed |
        Group-Object { Get-MissedFileKey -Mutant $_.mutant } |
        Sort-Object -Property @(
            @{ Expression = "Count"; Descending = $true },
            @{ Expression = "Name"; Descending = $false }
        ) |
        Select-Object -First 20 |
        ForEach-Object {
            [pscustomobject]@{
                file  = $_.Name
                count = $_.Count
            }
        }
)

$summaryPayload = [pscustomobject]@{
    run_id           = $aggregate.run_id
    campaign_root    = $aggregate.campaign_root
    generated_utc    = $aggregate.generated_utc
    shard_count      = $aggregate.shard_count
    totals           = [pscustomobject]$aggregate.totals
    top_missed_files = @($topMissedFiles)
    shards           = @($aggregate.shards)
    missed_mutants   = @($allMissed.ToArray())
    timeout_mutants  = @($allTimeout.ToArray())
    unviable_mutants = @($allUnviable.ToArray())
}
$summaryJsonPath = Join-Path $resolvedCampaignRoot "summary.json"
$summaryPayload | ConvertTo-Json -Depth 6 | Set-Content -Path $summaryJsonPath -Encoding utf8

$missedTextPath = Join-Path $resolvedCampaignRoot "missed.txt"
$timeoutTextPath = Join-Path $resolvedCampaignRoot "timeout.txt"
$unviableTextPath = Join-Path $resolvedCampaignRoot "unviable.txt"

@($allMissed | ForEach-Object { "{0}: {1}" -f $_.shard, $_.mutant }) | Set-Content -Path $missedTextPath -Encoding utf8
@($allTimeout | ForEach-Object { "{0}: {1}" -f $_.shard, $_.mutant }) | Set-Content -Path $timeoutTextPath -Encoding utf8
@($allUnviable | ForEach-Object { "{0}: {1}" -f $_.shard, $_.mutant }) | Set-Content -Path $unviableTextPath -Encoding utf8

$shardRows = @(
    foreach ($shard in $aggregate.shards) {
        "| {0} | {1} | {2} | {3} | {4} | {5} | {6} | {7} |" -f `
            $shard.shard, `
            $shard.status, `
            $(if ($null -eq $shard.total) { "?" } else { $shard.total }), `
            $shard.caught, `
            $shard.missed, `
            $shard.timeout, `
            $shard.unviable, `
            $(if ([string]::IsNullOrWhiteSpace([string]$shard.duration)) { "-" } else { $shard.duration })
    }
)

$topMissedRows = @(
    foreach ($entry in $topMissedFiles) {
        '- `{0}`: {1}' -f $entry.file, $entry.count
    }
)
if ($topMissedRows.Count -eq 0) {
    $topMissedRows = "- none"
}

$missedPreview = @(
    $allMissed |
        Select-Object -First 40 |
        ForEach-Object { '- [{0}] `{1}`' -f $_.shard, $_.mutant }
)
if ($missedPreview.Count -eq 0) {
    $missedPreview = "- none"
}

$timeoutPreview = @(
    $allTimeout |
        Select-Object -First 40 |
        ForEach-Object { '- [{0}] `{1}`' -f $_.shard, $_.mutant }
)
if ($timeoutPreview.Count -eq 0) {
    $timeoutPreview = "- none"
}

$summaryMarkdownPath = Join-Path $resolvedCampaignRoot "summary.md"
$summaryMarkdownLines = @(
    "# Mutation Campaign Summary"
    ""
    "- Run ID: $normalizedRunId"
    "- Campaign root: $resolvedCampaignRoot"
    "- Generated UTC: $($aggregate.generated_utc)"
    "- Shards found: $($aggregate.shard_count)"
    "- Total mutants: $($aggregate.totals.total)"
    "- Completed mutants: $($aggregate.totals.completed)"
    "- Caught: $($aggregate.totals.caught)"
    "- Missed: $($aggregate.totals.missed)"
    "- Timeout: $($aggregate.totals.timeout)"
    "- Unviable: $($aggregate.totals.unviable)"
    ""
    "## Shards"
    ""
    "| Shard | Status | Total | Caught | Missed | Timeout | Unviable | Duration |"
    "| --- | --- | ---: | ---: | ---: | ---: | ---: | --- |"
    $shardRows
    ""
    "## Top Missed Files"
    ""
    $topMissedRows
    ""
    "## Missed Preview"
    ""
    $missedPreview
    ""
    "Full missed list: missed.txt"
    ""
    "## Timeout Preview"
    ""
    $timeoutPreview
    ""
    "Full timeout list: timeout.txt"
    ""
    "## Artifacts"
    ""
    "- JSON summary: summary.json"
    "- Missed list: missed.txt"
    "- Timeout list: timeout.txt"
    "- Unviable list: unviable.txt"
)
Set-Content -Path $summaryMarkdownPath -Value $summaryMarkdownLines -Encoding utf8

Write-Host ("[mutants-summary] campaign_root={0}" -f $resolvedCampaignRoot)
Write-Host ("[mutants-summary] summary_markdown={0}" -f $summaryMarkdownPath)
Write-Host ("[mutants-summary] missed={0} timeout={1} unviable={2}" -f $aggregate.totals.missed, $aggregate.totals.timeout, $aggregate.totals.unviable)
