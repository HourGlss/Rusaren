[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$serverRoot = Split-Path -Parent $PSScriptRoot
$summaryPath = Join-Path $serverRoot "target\reports\coverage\summary.json"

if (-not (Test-Path $summaryPath)) {
    throw "Coverage summary was not found at $summaryPath. Run ./scripts/quality.ps1 reports first."
}

$coverageJson = Get-Content -Path $summaryPath -Raw | ConvertFrom-Json
$files = @($coverageJson.data[0].files)

$thresholds = @(
    @{ Path = "crates/game_api/src/app.rs"; MinLines = 80.0; MinFunctions = 80.0 },
    @{ Path = "crates/game_api/src/realtime.rs"; MinLines = 75.0; MinFunctions = 70.0 },
    @{ Path = "crates/game_api/src/webrtc.rs"; MinLines = 75.0; MinFunctions = 65.0 },
    @{ Path = "crates/game_domain/src/lib.rs"; MinLines = 85.0; MinFunctions = 85.0 },
    @{ Path = "crates/game_lobby/src/lib.rs"; MinLines = 85.0; MinFunctions = 85.0 },
    @{ Path = "crates/game_match/src/lib.rs"; MinLines = 85.0; MinFunctions = 85.0 },
    @{ Path = "crates/game_net/src/lib.rs"; MinLines = 85.0; MinFunctions = 85.0 },
    @{ Path = "crates/game_net/src/control.rs"; MinLines = 85.0; MinFunctions = 75.0 },
    @{ Path = "crates/game_net/src/ingress.rs"; MinLines = 85.0; MinFunctions = 85.0 },
    @{ Path = "crates/game_sim/src/lib.rs"; MinLines = 85.0; MinFunctions = 80.0 }
)

$results = foreach ($threshold in $thresholds) {
    $normalizedPath = $threshold.Path.Replace('/', '\')
    $file = $files | Where-Object { $_.filename -like "*$normalizedPath" } | Select-Object -First 1
    if ($null -eq $file) {
        throw "Coverage summary is missing an entry for $($threshold.Path)."
    }

    [pscustomobject]@{
        Path = $threshold.Path
        LinePercent = [double]$file.summary.lines.percent
        FunctionPercent = [double]$file.summary.functions.percent
        MinLines = [double]$threshold.MinLines
        MinFunctions = [double]$threshold.MinFunctions
    }
}

$failures = @(
    $results | Where-Object {
        $_.LinePercent -lt $_.MinLines -or $_.FunctionPercent -lt $_.MinFunctions
    }
)

$outputRoot = Join-Path $serverRoot "target\reports\coverage"
$outputPath = Join-Path $outputRoot "core-thresholds.txt"
$lines = @("Core runtime coverage thresholds")
foreach ($result in $results) {
    $lines += "{0}: lines {1:N2}% (min {2:N2}%), functions {3:N2}% (min {4:N2}%)" -f `
        $result.Path, $result.LinePercent, $result.MinLines, $result.FunctionPercent, $result.MinFunctions
}
Set-Content -Path $outputPath -Value $lines -Encoding utf8

if ($failures.Count -gt 0) {
    $messages = $failures | ForEach-Object {
        "{0} lines {1:N2}% / functions {2:N2}% below thresholds {3:N2}% / {4:N2}%" -f `
            $_.Path, $_.LinePercent, $_.FunctionPercent, $_.MinLines, $_.MinFunctions
    }
    throw "Core runtime coverage gate failed: $($messages -join '; ')"
}

Write-Host "Core runtime coverage gate passed."
