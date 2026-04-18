[CmdletBinding()]
param(
    [ValidateSet("all", "coverage", "complexity", "clean-code", "callgraph", "docs", "fuzz", "hardening", "frontend")]
    [string]$Report = "all",
    [switch]$FailOnCommandFailure
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
if ($PSVersionTable.PSVersion.Major -ge 7) {
    $PSNativeCommandUseErrorActionPreference = $true
}

$serverRoot = Split-Path -Parent $PSScriptRoot
$repoRoot = Split-Path -Parent $serverRoot
Set-Location $serverRoot

$cargoBin = Join-Path $HOME ".cargo\bin"
if (Test-Path $cargoBin) {
    $env:PATH = "$cargoBin$([System.IO.Path]::PathSeparator)$env:PATH"
}

$reportsRoot = Join-Path $serverRoot "target\reports"
$coverageRoot = Join-Path $reportsRoot "coverage"
$fuzzRoot = Join-Path $reportsRoot "fuzz"
$complexityRoot = Join-Path $reportsRoot "complexity"
$cleanCodeRoot = Join-Path $reportsRoot "clean-code"
$callgraphRoot = Join-Path $reportsRoot "callgraph"
$docsArtifactRoot = Join-Path $reportsRoot "docs"
$rustdocArtifactRoot = Join-Path $reportsRoot "rustdoc"
$hardeningRoot = Join-Path $reportsRoot "hardening"
$frontendRoot = Join-Path $reportsRoot "frontend"
$nightlyToolchain = if ([string]::IsNullOrWhiteSpace($env:RARENA_NIGHTLY_TOOLCHAIN)) {
    "nightly-2026-03-01"
}
else {
    $env:RARENA_NIGHTLY_TOOLCHAIN
}

function Escape-Html {
    param([AllowNull()][string]$Value)

    if ($null -eq $Value) {
        return ""
    }

    return [System.Net.WebUtility]::HtmlEncode($Value)
}

function Get-RustAnalyzerScipDiagnostics {
    param([string]$Path)

    if (-not (Test-Path $Path)) {
        return @()
    }

    return @(
        Get-Content -Path $Path |
            ForEach-Object { $_.TrimEnd() } |
            Where-Object {
                -not [string]::IsNullOrWhiteSpace($_) -and
                -not $_.StartsWith("rust-analyzer: ") -and
                -not $_.StartsWith("Generating SCIP ")
            }
    )
}

function Format-DiagnosticPreview {
    param(
        [string[]]$Lines,
        [int]$MaxLines = 6
    )

    if (($null -eq $Lines) -or ($Lines.Count -eq 0)) {
        return $null
    }

    $previewLines = @($Lines | Select-Object -First $MaxLines)
    $preview = $previewLines -join " | "
    if ($Lines.Count -gt $previewLines.Count) {
        $preview += " | ..."
    }

    return $preview
}

function Write-ReportHtml {
    param(
        [string]$Path,
        [string]$Title,
        [string]$Body
    )

    $document = @"
<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>$(Escape-Html $Title)</title>
<style>
:root {
    color-scheme: light;
    font-family: "Segoe UI", Tahoma, Geneva, Verdana, sans-serif;
    --bg: #f4f4f1;
    --panel: #ffffff;
    --ink: #17202a;
    --muted: #5f6b76;
    --line: #d6d9dc;
    --ok: #0f766e;
    --warn: #b45309;
    --bad: #b91c1c;
    --accent: #1d4ed8;
}

* {
    box-sizing: border-box;
}

body {
    margin: 0;
    background: linear-gradient(180deg, #fbfaf7 0%, #f1efe9 100%);
    color: var(--ink);
}

main {
    max-width: 1200px;
    margin: 0 auto;
    padding: 2rem 1.5rem 3rem;
}

h1,
h2,
h3 {
    margin: 0 0 0.75rem;
    line-height: 1.15;
}

p,
li {
    line-height: 1.5;
}

a {
    color: var(--accent);
}

.muted {
    color: var(--muted);
}

.panel {
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 18px;
    padding: 1rem 1.2rem;
    margin: 1rem 0;
    box-shadow: 0 10px 30px rgba(17, 24, 39, 0.05);
}

.grid {
    display: grid;
    gap: 1rem;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
}

.metric {
    background: #fcfcfb;
    border: 1px solid var(--line);
    border-radius: 14px;
    padding: 0.9rem 1rem;
}

.metric strong {
    display: block;
    font-size: 1.4rem;
    margin-top: 0.35rem;
}

.metric .detail {
    margin-top: 0.5rem;
    font-size: 0.9rem;
    color: var(--muted);
}

.badge {
    display: inline-block;
    border-radius: 999px;
    padding: 0.2rem 0.65rem;
    font-size: 0.85rem;
    font-weight: 700;
    letter-spacing: 0.02em;
}

.badge-ok {
    background: #d1fae5;
    color: #065f46;
}

.badge-warn {
    background: #fef3c7;
    color: #92400e;
}

.badge-bad {
    background: #fee2e2;
    color: #991b1b;
}

.badge-grade-a {
    background: #dcfce7;
    color: #166534;
}

.badge-grade-b {
    background: #d1fae5;
    color: #065f46;
}

.badge-grade-c {
    background: #fef3c7;
    color: #92400e;
}

.badge-grade-d {
    background: #fed7aa;
    color: #9a3412;
}

.badge-grade-e {
    background: #fecaca;
    color: #b91c1c;
}

.badge-grade-f {
    background: #fee2e2;
    color: #991b1b;
}

table {
    width: 100%;
    border-collapse: collapse;
    margin-top: 1rem;
    font-size: 0.96rem;
}

th,
td {
    text-align: left;
    padding: 0.75rem;
    border-bottom: 1px solid var(--line);
    vertical-align: top;
}

th {
    background: #f8fafc;
}

code {
    font-family: "Cascadia Code", Consolas, monospace;
    font-size: 0.92em;
}

.footer {
    margin-top: 2rem;
    font-size: 0.92rem;
}

@media (max-width: 720px) {
    main {
        padding: 1rem 0.85rem 2rem;
    }

    th,
    td {
        padding: 0.6rem;
    }
}
</style>
</head>
<body>
<main>
$Body
</main>
</body>
</html>
"@

    $directory = Split-Path -Parent $Path
    if (-not (Test-Path $directory)) {
        New-Item -ItemType Directory -Force -Path $directory | Out-Null
    }

    Set-Content -Path $Path -Value $document -Encoding UTF8
}

function Get-GitValue {
    param(
        [string[]]$CommandArgs,
        [string]$Fallback
    )

    try {
        $value = git @CommandArgs 2>$null | Select-Object -First 1
        if ([string]::IsNullOrWhiteSpace($value)) {
            return $Fallback
        }

        return $value.Trim()
    }
    catch {
        return $Fallback
    }
}

function Test-ToolAvailable {
    param([string]$CommandName)

    return $null -ne (Get-Command $CommandName -ErrorAction SilentlyContinue)
}

function Invoke-CheckedCommand {
    param(
        [Parameter(Mandatory)]
        [scriptblock]$Command,
        [Parameter(Mandatory)]
        [string]$Description
    )

    & $Command
    if ($LASTEXITCODE -ne 0) {
        throw "$Description failed with exit code $LASTEXITCODE."
    }
}

function Format-Percent {
    param([double]$Value)

    return ("{0:N2}%" -f $Value)
}

function Clamp-Score {
    param([double]$Value)

    return [math]::Max(0, [math]::Min(100, [math]::Round($Value, 2)))
}

function Get-PercentGrade {
    param([double]$Score)

    if ($Score -ge 90) {
        return "A"
    }
    elseif ($Score -ge 80) {
        return "B"
    }
    elseif ($Score -ge 70) {
        return "C"
    }
    elseif ($Score -ge 60) {
        return "D"
    }
    elseif ($Score -ge 50) {
        return "E"
    }

    return "F"
}

function Format-Score {
    param([double]$Score)

    return ("{0:N2}/100" -f (Clamp-Score -Value $Score))
}

function New-ScoreSummary {
    param(
        [double]$Score,
        [string]$Formula,
        [string[]]$Breakdown
    )

    $boundedScore = Clamp-Score -Value $Score
    return [pscustomobject]@{
        Score = $boundedScore
        Grade = Get-PercentGrade -Score $boundedScore
        Formula = $Formula
        Breakdown = @($Breakdown)
    }
}

function Get-OptionalPropertyValue {
    param(
        [Parameter(Mandatory)]
        [object]$InputObject,
        [Parameter(Mandatory)]
        [string]$PropertyName
    )

    $property = $InputObject.PSObject.Properties[$PropertyName]
    if ($null -eq $property) {
        return $null
    }

    return $property.Value
}

function Get-OptionalArrayPropertyValue {
    param(
        [Parameter(Mandatory)]
        [object]$InputObject,
        [Parameter(Mandatory)]
        [string]$PropertyName
    )

    $value = Get-OptionalPropertyValue -InputObject $InputObject -PropertyName $PropertyName
    if ($null -eq $value) {
        return ,@()
    }

    return ,@($value)
}

function Get-CoverageTupleValue {
    param(
        [AllowNull()]
        [object]$Tuple,
        [int]$Index
    )

    if ($null -eq $Tuple) {
        return 0.0
    }

    if ($Tuple -is [System.Array]) {
        if ($Index -lt $Tuple.Length) {
            return [double]$Tuple[$Index]
        }

        return 0.0
    }

    $valueProperty = $Tuple.PSObject.Properties["value"]
    if ($null -ne $valueProperty) {
        return Get-CoverageTupleValue -Tuple $valueProperty.Value -Index $Index
    }

    return 0.0
}

function Get-CoverageBranchSpanKey {
    param([AllowNull()][object]$BranchRecord)

    if ($null -eq $BranchRecord) {
        return $null
    }

    return "{0}:{1}-{2}:{3}:{4}" -f `
        [int](Get-CoverageTupleValue -Tuple $BranchRecord -Index 0), `
        [int](Get-CoverageTupleValue -Tuple $BranchRecord -Index 1), `
        [int](Get-CoverageTupleValue -Tuple $BranchRecord -Index 2), `
        [int](Get-CoverageTupleValue -Tuple $BranchRecord -Index 3), `
        [int](Get-CoverageTupleValue -Tuple $BranchRecord -Index 8)
}

function Get-CoverageEffectiveRegionMetrics {
    param([Parameter(Mandatory)][object]$CoverageFile)

    $regionSummary = $CoverageFile.summary.regions
    $regionCovered = [int]$regionSummary.covered
    $regionTotal = [int]$regionSummary.count
    $branchGroups = @{}

    $branchRecords = Get-OptionalPropertyValue -InputObject $CoverageFile -PropertyName "branches"
    foreach ($branchRecord in @($branchRecords)) {
        $branchKey = Get-CoverageBranchSpanKey -BranchRecord $branchRecord
        if ([string]::IsNullOrWhiteSpace($branchKey)) {
            continue
        }

        if (-not $branchGroups.ContainsKey($branchKey)) {
            $branchGroups[$branchKey] = [pscustomobject]@{
                TrueCount = 0.0
                FalseCount = 0.0
            }
        }

        $branchGroups[$branchKey].TrueCount += [double](Get-CoverageTupleValue -Tuple $branchRecord -Index 4)
        $branchGroups[$branchKey].FalseCount += [double](Get-CoverageTupleValue -Tuple $branchRecord -Index 5)
    }

    $branchRegionTotal = 0
    $branchRegionCoveredBinary = 0
    $branchOutcomeTotal = 0
    $branchOutcomeCovered = 0
    foreach ($branchGroup in $branchGroups.Values) {
        $branchRegionTotal += 1
        $branchOutcomeTotal += 2

        if (($branchGroup.TrueCount + $branchGroup.FalseCount) -gt 0) {
            $branchRegionCoveredBinary += 1
        }
        if ($branchGroup.TrueCount -gt 0) {
            $branchOutcomeCovered += 1
        }
        if ($branchGroup.FalseCount -gt 0) {
            $branchOutcomeCovered += 1
        }
    }

    $effectiveRegionTotal = $regionTotal - $branchRegionTotal + $branchOutcomeTotal
    $effectiveRegionCovered = $regionCovered - $branchRegionCoveredBinary + $branchOutcomeCovered
    $effectiveRegionPercent = if ($effectiveRegionTotal -gt 0) {
        ([double]$effectiveRegionCovered / [double]$effectiveRegionTotal) * 100.0
    }
    else {
        0.0
    }

    return [pscustomobject]@{
        RawRegionCovered = $regionCovered
        RawRegionTotal = $regionTotal
        RawRegionPercent = if ($regionTotal -gt 0) {
            ([double]$regionCovered / [double]$regionTotal) * 100.0
        }
        else {
            0.0
        }
        BranchRegionTotal = $branchRegionTotal
        BranchRegionCoveredBinary = $branchRegionCoveredBinary
        BranchOutcomeTotal = $branchOutcomeTotal
        BranchOutcomeCovered = $branchOutcomeCovered
        EffectiveRegionCovered = $effectiveRegionCovered
        EffectiveRegionTotal = $effectiveRegionTotal
        EffectiveRegionPercent = $effectiveRegionPercent
    }
}

function Get-BackendCoreRuntimeFiles {
    param(
        [Parameter(Mandatory)]
        [hashtable]$SourceInventory
    )

    return @(
        $SourceInventory.Values |
            Where-Object {
                $_.IsRuntimeSource -and
                $_.NormalizedPath -match '^crates/(game_api|game_domain|game_lobby|game_match|game_net|game_sim)/src/.+\.rs$'
            } |
            ForEach-Object { $_.NormalizedPath } |
            Sort-Object -Unique
    )
}

function Get-StatusBadgeClass {
    param([string]$Status)

    switch ($Status) {
        "ok" { return "badge-ok" }
        "warning" { return "badge-warn" }
        default { return "badge-bad" }
    }
}

function Get-CyclomaticGrade {
    param([double]$Score)

    if ($Score -le 5) {
        return "A"
    }
    elseif ($Score -le 10) {
        return "B"
    }
    elseif ($Score -le 20) {
        return "C"
    }
    elseif ($Score -le 30) {
        return "D"
    }
    elseif ($Score -le 40) {
        return "E"
    }

    return "F"
}

function Get-MaintainabilityGrade {
    param([double]$Score)

    if ($Score -gt 19) {
        return "A"
    }
    elseif ($Score -gt 9) {
        return "B"
    }

    return "C"
}

function Get-GradeBadgeClass {
    param([string]$Grade)

    switch ($Grade) {
        "A" { return "badge-grade-a" }
        "B" { return "badge-grade-b" }
        "C" { return "badge-grade-c" }
        "D" { return "badge-grade-d" }
        "E" { return "badge-grade-e" }
        "F" { return "badge-grade-f" }
        default { return "badge-warn" }
    }
}

function Get-GradeSeverity {
    param([string]$Grade)

    switch ($Grade) {
        "A" { return 1 }
        "B" { return 2 }
        "C" { return 3 }
        "D" { return 4 }
        "E" { return 5 }
        "F" { return 6 }
        default { return 0 }
    }
}

function Get-CyclomaticGradeScore {
    param([string]$Grade)

    switch ($Grade) {
        "A" { return 100.0 }
        "B" { return 85.0 }
        "C" { return 70.0 }
        "D" { return 55.0 }
        "E" { return 40.0 }
        "F" { return 20.0 }
        default { return 0.0 }
    }
}

function Format-GradeBadge {
    param([string]$Grade)

    if ([string]::IsNullOrWhiteSpace($Grade)) {
        return '<span class="muted">n/a</span>'
    }

    return '<span class="badge {0}">{1}</span>' -f (Get-GradeBadgeClass -Grade $Grade), (Escape-Html $Grade)
}

function Convert-ToDisplayPath {
    param([string]$Path)

    if ([string]::IsNullOrWhiteSpace($Path)) {
        return ""
    }

    try {
        $fullPath = [System.IO.Path]::GetFullPath($Path)
        if ($fullPath.StartsWith($serverRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
            return $fullPath.Substring($serverRoot.Length).TrimStart('\', '/')
        }

        if ($fullPath.StartsWith($repoRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
            return $fullPath.Substring($repoRoot.Length).TrimStart('\', '/')
        }
    }
    catch {
        return $Path
    }

    return $Path
}

function Get-NormalizedDisplayPath {
    param([string]$Path)

    return (Convert-ToDisplayPath -Path $Path) -replace '\\', '/'
}

function Test-IsRuntimeSourcePath {
    param([string]$Path)

    $normalizedPath = Get-NormalizedDisplayPath -Path $Path
    return ($normalizedPath -like "crates/*/src/*.rs") -and -not (Test-IsTestSourcePath -Path $Path)
}

function Test-IsTestSourcePath {
    param([string]$Path)

    $normalizedPath = Get-NormalizedDisplayPath -Path $Path
    return (
        ($normalizedPath -like "crates/*/tests/*.rs") -or
        ($normalizedPath -match '^crates/[^/]+/src/(tests\.rs|tests/.+\.rs|.+/tests\.rs|.+/tests/.+\.rs)$') -or
        ($normalizedPath -match '^bin/[^/]+/src/(tests\.rs|tests/.+\.rs|.+/tests\.rs|.+/tests/.+\.rs)$')
    )
}

function Test-IsEntryPointSourcePath {
    param([string]$Path)

    $normalizedPath = Get-NormalizedDisplayPath -Path $Path
    return $normalizedPath -eq "bin/dedicated_server/src/main.rs"
}

function Test-IsScoredCodePath {
    param([string]$Path)

    $normalizedPath = Get-NormalizedDisplayPath -Path $Path
    return (
        -not (Test-IsTestSourcePath -Path $Path) -and
        -not ($normalizedPath -like "fuzz/*")
    )
}

function Test-IsToolingSourcePath {
    param([string]$Path)

    return -not (Test-IsRuntimeSourcePath -Path $Path) -and -not (Test-IsTestSourcePath -Path $Path) -and -not (Test-IsEntryPointSourcePath -Path $Path)
}

function Get-SourceCategoryLabel {
    param($SourceInfo)

    if ($null -eq $SourceInfo) {
        return "Unknown"
    }

    if ($SourceInfo.IsScoredCodeSource) {
        return "Code"
    }
    if ($SourceInfo.IsRuntimeSource) {
        return "Runtime"
    }
    if ($SourceInfo.IsEntryPointSource) {
        return "Entrypoint"
    }
    if ($SourceInfo.IsTestSource) {
        return "Test"
    }
    if ($SourceInfo.IsToolingSource) {
        return "Tooling"
    }

    return "Other"
}

function Get-SourceInventory {
    $inventory = @{}
    $roots = @(
        (Join-Path $serverRoot "crates"),
        (Join-Path $serverRoot "bin")
    )

    foreach ($root in $roots) {
        if (-not (Test-Path $root)) {
            continue
        }

        foreach ($file in Get-ChildItem -Path $root -Recurse -File -Filter *.rs) {
            $content = Get-Content -Path $file.FullName -Raw
            $lines = Get-Content -Path $file.FullName
            $meaningfulLines = @(
                $lines |
                    ForEach-Object { $_.Trim() } |
                    Where-Object {
                        $_ -and
                        -not $_.StartsWith("//") -and
                        -not $_.StartsWith("#![")
                    }
            )

            $hasExecutableSurface = $content -match '\b(fn|struct|enum|impl|trait|const|type)\b'
            $hasInlineTests = $content -match '(?m)#\s*\[\s*test\s*\]'
            $isPlaceholder = (-not $hasExecutableSurface) -and ($meaningfulLines.Count -le 2)
            $displayPath = Convert-ToDisplayPath -Path $file.FullName
            $normalizedPath = $displayPath -replace '\\', '/'
            $isTestSource = Test-IsTestSourcePath -Path $displayPath
            $isRuntimeSource = (-not $isTestSource) -and ($normalizedPath -like "crates/*/src/*.rs")
            $isEntryPointSource = Test-IsEntryPointSourcePath -Path $displayPath
            $isToolingSource = (-not $isRuntimeSource) -and (-not $isTestSource) -and (-not $isEntryPointSource)
            $isScoredCodeSource = Test-IsScoredCodePath -Path $displayPath

            $inventory[$displayPath] = [pscustomobject]@{
                DisplayPath = $displayPath
                NormalizedPath = $normalizedPath
                LineCount = @($lines).Count
                MeaningfulLineCount = $meaningfulLines.Count
                IsScoredCodeSource = $isScoredCodeSource
                IsRuntimeSource = $isRuntimeSource
                IsTestSource = $isTestSource
                IsEntryPointSource = $isEntryPointSource
                IsToolingSource = $isToolingSource
                IsPlaceholder = $isPlaceholder
                HasInlineTests = $hasInlineTests
                Reason = if ($isPlaceholder) {
                    "Only crate-level docs and attributes exist here; there is no substantive executable logic to cover yet."
                }
                elseif (-not $hasInlineTests) {
                    "No inline #[test] functions exist in this file; coverage may instead come from integration tests, end-to-end tests, or fuzz corpus replay."
                }
                else {
                    $null
                }
            }
        }
    }

    return $inventory
}

function Get-CleanCodeLineScore {
    param($SourceInfo)

    $lineCount = [double]$SourceInfo.LineCount

    if ($SourceInfo.IsScoredCodeSource) {
        if ($lineCount -le 250) { return 100.0 }
        elseif ($lineCount -le 400) { return 90.0 }
        elseif ($lineCount -le 600) { return 75.0 }
        elseif ($lineCount -le 800) { return 60.0 }
        elseif ($lineCount -le 1000) { return 45.0 }
        elseif ($lineCount -le 1400) { return 30.0 }
        return 15.0
    }

    if ($SourceInfo.IsTestSource) {
        if ($lineCount -le 250) { return 100.0 }
        elseif ($lineCount -le 400) { return 92.0 }
        elseif ($lineCount -le 600) { return 82.0 }
        elseif ($lineCount -le 900) { return 68.0 }
        elseif ($lineCount -le 1200) { return 52.0 }
        elseif ($lineCount -le 1800) { return 35.0 }
        return 20.0
    }

    if ($lineCount -le 250) { return 100.0 }
    elseif ($lineCount -le 500) { return 85.0 }
    elseif ($lineCount -le 900) { return 65.0 }
    return 40.0
}

function Get-CleanCodeSizeBand {
    param($SourceInfo)

    $lineCount = [int]$SourceInfo.LineCount

    if ($SourceInfo.IsScoredCodeSource) {
        if ($lineCount -le 400) { return "compact" }
        elseif ($lineCount -le 800) { return "large" }
        return "oversized"
    }

    if ($SourceInfo.IsTestSource) {
        if ($lineCount -le 400) { return "compact" }
        elseif ($lineCount -le 1200) { return "large" }
        return "oversized"
    }

    if ($lineCount -le 500) { return "compact" }
    elseif ($lineCount -le 900) { return "large" }
    return "oversized"
}

function Invoke-ClippyAnalysisSummary {
    $warnings = [System.Collections.Generic.List[string]]::new()
    $notes = [System.Collections.Generic.List[string]]::new()
    $commandText = "rustup run stable cargo clippy --workspace --all-targets --message-format json"
    $stdoutPath = [System.IO.Path]::GetTempFileName()
    $stderrPath = [System.IO.Path]::GetTempFileName()
    $output = @()
    $stderrOutput = @()

    try {
        $process = Start-Process `
            -FilePath "rustup" `
            -ArgumentList @(
                "run", "stable",
                "cargo", "clippy",
                "--workspace",
                "--all-targets",
                "--message-format", "json"
            ) `
            -WorkingDirectory $serverRoot `
            -NoNewWindow `
            -Wait `
            -PassThru `
            -RedirectStandardOutput $stdoutPath `
            -RedirectStandardError $stderrPath
        $exitCode = $process.ExitCode

        if (Test-Path $stdoutPath) {
            $output = @(Get-Content -Path $stdoutPath)
        }
        if (Test-Path $stderrPath) {
            $stderrOutput = @(Get-Content -Path $stderrPath)
        }
    }
    finally {
        foreach ($path in @($stdoutPath, $stderrPath)) {
            if (Test-Path $path) {
                Remove-Item -Force -Path $path
            }
        }
    }

    foreach ($line in $output) {
        if ($line -isnot [string]) {
            continue
        }
        $trimmed = $line.Trim()
        if (-not $trimmed.StartsWith("{")) {
            continue
        }

        try {
            $message = $trimmed | ConvertFrom-Json -ErrorAction Stop
        }
        catch {
            continue
        }

        if ($message.reason -ne "compiler-message") {
            continue
        }

        $level = [string]$message.message.level
        if ($level -ne "warning") {
            continue
        }

        $rendered = [string]$message.message.rendered
        if ([string]::IsNullOrWhiteSpace($rendered)) {
            $rendered = [string]$message.message.message
        }
        if (-not [string]::IsNullOrWhiteSpace($rendered)) {
            $warnings.Add(($rendered -replace '\s+$', ''))
        }
    }

    if ($warnings.Count -eq 0) {
        $notes.Add("cargo clippy reported zero warnings across the workspace and all targets.")
    }
    else {
        $notes.Add("cargo clippy reported $($warnings.Count) warning(s) across the workspace and all targets.")
    }

    if ($exitCode -ne 0) {
        $notes.Add("cargo clippy exited non-zero; the static-analysis score is penalized until the workspace is lint-clean again.")
        $diagnosticTail = @($stderrOutput | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Select-Object -Last 3)
        foreach ($line in $diagnosticTail) {
            $notes.Add($line.Trim())
        }
    }

    $warningPenalty = [math]::Min(60.0, [double]$warnings.Count * 4.0)
    $score = if ($exitCode -eq 0) {
        Clamp-Score -Value (100.0 - $warningPenalty)
    }
    else {
        Clamp-Score -Value (70.0 - $warningPenalty)
    }

    return [pscustomobject]@{
        Status = if ($exitCode -eq 0) { "ok" } else { "failed" }
        ExitCode = $exitCode
        WarningCount = $warnings.Count
        Score = $score
        Grade = Get-PercentGrade -Score $score
        Command = $commandText
        Notes = @($notes)
        SampleWarnings = @($warnings | Select-Object -First 5)
    }
}

function Invoke-CleanCodeReport {
    param(
        [hashtable]$SourceInventory
    )

    $notes = [System.Collections.Generic.List[string]]::new()
    $reportPath = Join-Path $cleanCodeRoot "index.html"
    $outputPath = Join-Path $cleanCodeRoot "output.html"
    $summaryPath = Join-Path $cleanCodeRoot "summary.json"

    try {
        if (Test-Path $cleanCodeRoot) {
            Remove-Item -Recurse -Force -Path $cleanCodeRoot
        }

        $allFiles = @(
            $SourceInventory.Values |
                Where-Object { -not $_.IsPlaceholder } |
                Sort-Object @{ Expression = "LineCount"; Descending = $true }, DisplayPath
        )

        $scoredFiles = foreach ($source in $allFiles) {
            $lineScore = Get-CleanCodeLineScore -SourceInfo $source
            $penalty = 0.0
            $reasons = [System.Collections.Generic.List[string]]::new()
            $category = Get-SourceCategoryLabel -SourceInfo $source
            $sizeBand = Get-CleanCodeSizeBand -SourceInfo $source

            if ($source.IsScoredCodeSource -and $source.HasInlineTests) {
                $penalty += 35.0
                $reasons.Add("Inline #[test] functions are still mixed into production code.")
            }

            if ($sizeBand -eq "oversized") {
                $reasons.Add("This file is still beyond the repo's target reviewable size for its category.")
            }
            elseif ($sizeBand -eq "large") {
                $reasons.Add("This file is larger than ideal and should keep trending downward.")
            }

            if ($source.IsScoredCodeSource -and $source.LineCount -gt 1000) {
                $penalty += 10.0
            }
            if ($source.IsTestSource -and $source.LineCount -gt 1800) {
                $penalty += 10.0
            }

            [pscustomobject]@{
                DisplayPath = $source.DisplayPath
                Category = $category
                LineCount = [int]$source.LineCount
                MeaningfulLineCount = [int]$source.MeaningfulLineCount
                HasInlineTests = [bool]$source.HasInlineTests
                SizeBand = $sizeBand
                Score = Clamp-Score -Value ($lineScore - $penalty)
                Grade = Get-PercentGrade -Score (Clamp-Score -Value ($lineScore - $penalty))
                Reasons = @($reasons)
            }
        }

        $codeFiles = @($scoredFiles | Where-Object { $_.Category -eq "Code" })
        $testFiles = @($scoredFiles | Where-Object { $_.Category -eq "Test" })
        $supplementalFiles = @($scoredFiles | Where-Object { $_.Category -in @("Tooling", "Other", "Unknown") })

        $codeAverage = if ($codeFiles.Count -gt 0) {
            [double](($codeFiles | Measure-Object -Property Score -Average).Average)
        }
        else {
            100.0
        }
        $testAverage = if ($testFiles.Count -gt 0) {
            [double](($testFiles | Measure-Object -Property Score -Average).Average)
        }
        else {
            100.0
        }
        $compactCodePercent = if ($codeFiles.Count -gt 0) {
            ((@($codeFiles | Where-Object { $_.LineCount -le 600 -and -not $_.HasInlineTests }).Count) / $codeFiles.Count) * 100.0
        }
        else {
            100.0
        }

        $structureScoreSummary = New-ScoreSummary `
            -Score (($codeAverage * 0.7) + ($testAverage * 0.2) + ($compactCodePercent * 0.1)) `
            -Formula "70% average non-test code structure score + 20% average test structure score + 10% non-test code files at <=600 lines without inline tests" `
            -Breakdown @(
                "Average non-test code structure score: $("{0:N2}" -f $codeAverage)",
                "Average test structure score: $("{0:N2}" -f $testAverage)",
                "Non-test code files at <=600 lines without inline tests: $(Format-Percent -Value $compactCodePercent)"
            )
        $clippySummary = Invoke-ClippyAnalysisSummary
        $scoreSummary = New-ScoreSummary `
            -Score (($structureScoreSummary.Score * 0.8) + ($clippySummary.Score * 0.2)) `
            -Formula "80% structural clean-code score + 20% cargo clippy static-analysis score" `
            -Breakdown @(
                "Structural clean-code score: $("{0:N2}" -f $structureScoreSummary.Score)",
                "cargo clippy static-analysis score: $("{0:N2}" -f $clippySummary.Score) (warnings: $($clippySummary.WarningCount), exit code: $($clippySummary.ExitCode))"
            )

        $oversizedCodeFiles = @($codeFiles | Where-Object { $_.SizeBand -eq "oversized" })
        $oversizedTestFiles = @($testFiles | Where-Object { $_.SizeBand -eq "oversized" })
        $codeInlineTests = @($codeFiles | Where-Object { $_.HasInlineTests })
        $largestFile = $scoredFiles | Select-Object -First 1

        $notes.Add("This report is heuristic. It scores structural cleanliness signals: file size, production/test separation, and whether the largest files are still trending toward smaller modules.")
        $notes.Add("Clippy and the complexity report remain the primary logic-level signals. This report complements them by grading file-level structure.")
        $notes.Add("All non-test Rust code under crates/ and bin/ now uses the same clean-code thresholds and contributes equally to the headline score.")
        $notes.Add("Inline #[test] functions inside non-test code incur a direct penalty because they mix production and test concerns.")
        $notes.Add("A file can still be perfectly valid Rust and pass tests while scoring poorly here if it is too large or blends multiple responsibilities.")
        foreach ($note in $clippySummary.Notes) {
            $notes.Add($note)
        }

        $codeRows = foreach ($file in ($codeFiles | Sort-Object @{ Expression = "Score"; Ascending = $true }, @{ Expression = "LineCount"; Descending = $true }, DisplayPath | Select-Object -First 20)) {
            $reasons = if ($file.Reasons.Count -gt 0) { Escape-Html ($file.Reasons -join " | ") } else { "Healthy for its category." }
            @"
<tr>
  <td><code>$(Escape-Html $file.DisplayPath)</code></td>
  <td>$(Escape-Html $file.Category)</td>
  <td>$($file.LineCount)</td>
  <td>$($file.MeaningfulLineCount)</td>
  <td>$(Escape-Html $file.SizeBand)</td>
  <td>$(if ($file.HasInlineTests) { '<span class="badge badge-bad">yes</span>' } else { '<span class="badge badge-ok">no</span>' })</td>
  <td>$(Format-Score -Score $file.Score) $(Format-GradeBadge -Grade $file.Grade)</td>
  <td>$reasons</td>
</tr>
"@
        }

        $testRows = foreach ($file in ($testFiles | Sort-Object @{ Expression = "Score"; Ascending = $true }, @{ Expression = "LineCount"; Descending = $true }, DisplayPath | Select-Object -First 20)) {
            $reasons = if ($file.Reasons.Count -gt 0) { Escape-Html ($file.Reasons -join " | ") } else { "Healthy for its category." }
            @"
<tr>
  <td><code>$(Escape-Html $file.DisplayPath)</code></td>
  <td>$($file.LineCount)</td>
  <td>$($file.MeaningfulLineCount)</td>
  <td>$(Escape-Html $file.SizeBand)</td>
  <td>$(Format-Score -Score $file.Score) $(Format-GradeBadge -Grade $file.Grade)</td>
  <td>$reasons</td>
</tr>
"@
        }

        $supplementalRows = foreach ($file in ($supplementalFiles | Sort-Object @{ Expression = "LineCount"; Descending = $true }, DisplayPath | Select-Object -First 15)) {
            @"
<tr>
  <td><code>$(Escape-Html $file.DisplayPath)</code></td>
  <td>$(Escape-Html $file.Category)</td>
  <td>$($file.LineCount)</td>
  <td>$($file.MeaningfulLineCount)</td>
  <td>$(Escape-Html $file.SizeBand)</td>
  <td>$(Format-Score -Score $file.Score) $(Format-GradeBadge -Grade $file.Grade)</td>
</tr>
"@
        }

        $noteItems = foreach ($note in ($notes | Sort-Object -Unique)) {
            "<li>$(Escape-Html $note)</li>"
        }

        $body = @"
<h1>Clean Code Report</h1>
<p class="muted">This report grades structural cleanliness heuristics for the Rust codebase. All non-test Rust code under <code>crates/</code> and <code>bin/</code> is now scored by the same size and separation rules.</p>
<div class="grid">
  <div class="metric"><span class="muted">Clean-code score</span><strong>$(Format-Score -Score $scoreSummary.Score) $(Format-GradeBadge -Grade $scoreSummary.Grade)</strong><div class="detail">$(Escape-Html $scoreSummary.Formula)</div></div>
  <div class="metric"><span class="muted">Scored code files</span><strong>$($codeFiles.Count)</strong><div class="detail">$($oversizedCodeFiles.Count) oversized</div></div>
  <div class="metric"><span class="muted">Test files</span><strong>$($testFiles.Count)</strong><div class="detail">$($oversizedTestFiles.Count) oversized</div></div>
  <div class="metric"><span class="muted">Code files with inline tests</span><strong>$($codeInlineTests.Count)</strong></div>
  <div class="metric"><span class="muted">Largest tracked file</span><strong>$(if ($null -ne $largestFile) { Escape-Html $largestFile.DisplayPath } else { 'n/a' })</strong><div class="detail">$(if ($null -ne $largestFile) { "$($largestFile.LineCount) lines" } else { "" })</div></div>
  <div class="metric"><span class="muted">Compact code coverage</span><strong>$(Format-Percent -Value $compactCodePercent)</strong><div class="detail"><=600 lines with no inline tests</div></div>
  <div class="metric"><span class="muted">cargo clippy</span><strong>$(Format-Score -Score $clippySummary.Score) $(Format-GradeBadge -Grade $clippySummary.Grade)</strong><div class="detail">$($clippySummary.WarningCount) warnings, exit code $($clippySummary.ExitCode)</div></div>
</div>
<div class="panel">
  <h2>Rust Static Analysis</h2>
  <p><code>$(Escape-Html $clippySummary.Command)</code></p>
  <p class="muted">cargo clippy is the Rust-native static-analysis signal in this report. It complements the structural score instead of replacing it.</p>
  $(if ($clippySummary.SampleWarnings.Count -gt 0) {
@"
  <pre><code>$(Escape-Html ($clippySummary.SampleWarnings -join "`n`n"))</code></pre>
"@
} else {
@"
  <p>No clippy warnings were reported.</p>
"@
})
</div>
<div class="panel">
  <h2>Scored non-test code files</h2>
  <table>
    <thead>
      <tr>
        <th>File</th>
        <th>Category</th>
        <th>Lines</th>
        <th>Meaningful lines</th>
        <th>Size band</th>
        <th>Inline tests</th>
        <th>Score</th>
        <th>Why it is here</th>
      </tr>
    </thead>
    <tbody>
$(($codeRows -join "`n"))
    </tbody>
  </table>
</div>
<div class="panel">
  <h2>Largest test files</h2>
  <table>
    <thead>
      <tr>
        <th>File</th>
        <th>Lines</th>
        <th>Meaningful lines</th>
        <th>Size band</th>
        <th>Score</th>
        <th>Why it is here</th>
      </tr>
    </thead>
    <tbody>
$(($testRows -join "`n"))
    </tbody>
  </table>
</div>
<div class="panel">
  <h2>Supplemental tooling and other files</h2>
  <table>
    <thead>
      <tr>
        <th>File</th>
        <th>Category</th>
        <th>Lines</th>
        <th>Meaningful lines</th>
        <th>Size band</th>
        <th>Score</th>
      </tr>
    </thead>
    <tbody>
$(($supplementalRows -join "`n"))
    </tbody>
  </table>
</div>
<div class="panel">
  <h2>How to read this score</h2>
  <ul>
$(($noteItems -join "`n"))
  </ul>
</div>
<p class="footer"><a href="../index.html">Back to report index</a></p>
"@

        Write-ReportHtml -Path $reportPath -Title "Clean Code Report" -Body $body
        Write-ReportHtml -Path $outputPath -Title "Clean Code Report" -Body $body

        $summaryPayload = [pscustomobject]@{
            commit = Get-GitValue -CommandArgs @("rev-parse", "--short", "HEAD") -Fallback "unknown"
            score = [pscustomobject]@{
                value = [math]::Round([double]$scoreSummary.Score, 2)
                grade = $scoreSummary.Grade
                formula = $scoreSummary.Formula
                breakdown = @($scoreSummary.Breakdown)
            }
            static_analysis = [pscustomobject]@{
                tool = "cargo clippy"
                status = $clippySummary.Status
                exit_code = $clippySummary.ExitCode
                score = [math]::Round([double]$clippySummary.Score, 2)
                grade = $clippySummary.Grade
                warning_count = $clippySummary.WarningCount
                command = $clippySummary.Command
                sample_warnings = @($clippySummary.SampleWarnings)
            }
            files = [pscustomobject]@{
                code = $codeFiles.Count
                tests = $testFiles.Count
                supplemental = $supplementalFiles.Count
                oversized_code = $oversizedCodeFiles.Count
                oversized_tests = $oversizedTestFiles.Count
                code_inline_tests = $codeInlineTests.Count
            }
            largest = if ($null -ne $largestFile) {
                [pscustomobject]@{
                    path = $largestFile.DisplayPath
                    category = $largestFile.Category
                    line_count = $largestFile.LineCount
                    meaningful_line_count = $largestFile.MeaningfulLineCount
                    score = $largestFile.Score
                    grade = $largestFile.Grade
                }
            } else { $null }
            notes = @($notes | Sort-Object -Unique)
        }
        $summaryPayload | ConvertTo-Json -Depth 6 | Set-Content -Path $summaryPath -Encoding UTF8

        return [pscustomobject]@{
            Name = "Clean Code"
            Status = "ok"
            Notes = @($notes | Sort-Object -Unique)
            IndexPath = "clean-code/index.html"
            ErrorMessage = $null
            ScoreSummary = $scoreSummary
        }
    }
    catch {
        $errorMessage = $_.Exception.Message
        $notes.Add("Clean-code report generation failed: $errorMessage")
        $body = @"
<h1>Clean Code Report Failed</h1>
<div class="panel">
  <p>The clean-code report could not be generated.</p>
  <p><code>$(Escape-Html $errorMessage)</code></p>
</div>
<p class="footer"><a href="../index.html">Back to report index</a></p>
"@
        Write-ReportHtml -Path $reportPath -Title "Clean Code Report Failed" -Body $body
        Write-ReportHtml -Path $outputPath -Title "Clean Code Report Failed" -Body $body

        return [pscustomobject]@{
            Name = "Clean Code"
            Status = "failed"
            Notes = @($notes)
            IndexPath = "clean-code/index.html"
            ErrorMessage = $errorMessage
        }
    }
}

function Get-ComplexityFunctions {
    param(
        [Parameter(Mandatory)]
        $Node,
        [string]$FilePath,
        [string]$ParentPath = ""
    )

    $items = @()
    $currentPath = $ParentPath

    if ($Node.kind -eq "function") {
        $name = if ([string]::IsNullOrWhiteSpace($ParentPath)) {
            [string]$Node.name
        }
        else {
            "$ParentPath::$($Node.name)"
        }

        $items += [pscustomobject]@{
            FilePath = $FilePath
            Name = $name
            StartLine = [int]$Node.start_line
            EndLine = [int]$Node.end_line
            Cognitive = [double]$Node.metrics.cognitive.sum
            Cyclomatic = [double]$Node.metrics.cyclomatic.sum
            Mi = [double]$Node.metrics.mi.mi_visual_studio
            Sloc = [double]$Node.metrics.loc.sloc
        }
    }
    elseif (-not [string]::IsNullOrWhiteSpace([string]$Node.name) -and $Node.kind -ne "unit") {
        $currentPath = if ([string]::IsNullOrWhiteSpace($ParentPath)) {
            [string]$Node.name
        }
        else {
            "$ParentPath::$($Node.name)"
        }
    }

    foreach ($child in @($Node.spaces)) {
        $items += Get-ComplexityFunctions -Node $child -FilePath $FilePath -ParentPath $currentPath
    }

    return $items
}

function Get-BackendCorePrompt {
    param(
        [string]$FilePath,
        [object[]]$HotFunctions,
        [double]$FuzzLinePercent,
        [double]$FuzzFunctionPercent
    )

    $functionNames = @($HotFunctions | Select-Object -First 3 | ForEach-Object { $_.Name })
    $functionList = if ($functionNames.Count -gt 0) {
        $functionNames -join ", "
    }
    else {
        "the most complex functions in this file"
    }

    $isNetworkFacing = ($FilePath -replace '\\', '/') -like "crates/game_net/*" -or ($FilePath -replace '\\', '/') -eq "crates/game_api/src/realtime.rs"
    if ($isNetworkFacing) {
        return "Refactor $FilePath to reduce complexity in $functionList without changing packet formats or externally visible behavior. Split decode, validation, and dispatch branches into smaller helpers. Add positive and negative tests for each touched branch and extend fuzz seeds or corpus replay coverage because current fuzz coverage is $(Format-Percent -Value $FuzzLinePercent) lines and $(Format-Percent -Value $FuzzFunctionPercent) functions."
    }

    return "Refactor $FilePath to reduce complexity in $functionList without changing externally visible behavior. Split branching logic into smaller helpers, add focused positive and negative tests for the touched branches, and keep the file easier to reason about before adding more features."
}

function Get-FuzzTargetCatalog {
    return @(
        [pscustomobject]@{
            Target = "packet_header_decode"
            Scope = "crates/game_net/src/header.rs plus packet kind/channel decoding in crates/game_net/src/packet_types.rs."
            Description = "Packet framing and header validation."
            Paths = @(
                "crates/game_net/src/header.rs",
                "crates/game_net/src/packet_types.rs"
            )
            Primary = $true
        },
        [pscustomobject]@{
            Target = "control_command_decode"
            Scope = "crates/game_net/src/control/client.rs and crates/game_net/src/control/codec.rs plus domain validation via decoded identifiers and names."
            Description = "Control command decode and validation."
            Paths = @(
                "crates/game_net/src/control/client.rs",
                "crates/game_net/src/control/codec.rs"
            )
            Primary = $true
        },
        [pscustomobject]@{
            Target = "input_frame_decode"
            Scope = "crates/game_net/src/input.rs plus header framing in crates/game_net/src/header.rs and crates/game_net/src/packet_types.rs."
            Description = "Input packet decode and button/context validation."
            Paths = @(
                "crates/game_net/src/input.rs",
                "crates/game_net/src/header.rs",
                "crates/game_net/src/packet_types.rs"
            )
            Primary = $true
        },
        [pscustomobject]@{
            Target = "session_ingress"
            Scope = "crates/game_net/src/ingress.rs plus control command decode in crates/game_net/src/control/client.rs and crates/game_net/src/control/codec.rs."
            Description = "Session binding and hostile ingress sequencing."
            Paths = @(
                "crates/game_net/src/ingress.rs",
                "crates/game_net/src/control/client.rs",
                "crates/game_net/src/control/codec.rs"
            )
            Primary = $true
        },
        [pscustomobject]@{
            Target = "session_ingress_sequence"
            Scope = "crates/game_net/src/ingress.rs plus structured control command ingress sequencing through crates/game_net/src/control/client.rs and crates/game_net/src/control/codec.rs."
            Description = "Structure-aware ingress packet sequencing over valid, truncated, oversized, and wrong-kind control packets."
            Paths = @(
                "crates/game_net/src/ingress.rs",
                "crates/game_net/src/control/client.rs",
                "crates/game_net/src/control/codec.rs"
            )
            Primary = $false
        },
        [pscustomobject]@{
            Target = "server_control_event_decode"
            Scope = "crates/game_net/src/control/server_decode.rs and crates/game_net/src/control/snapshots_decode.rs plus codec helpers."
            Description = "Server control event decode for lobby directory and full lobby snapshot payloads."
            Paths = @(
                "crates/game_net/src/control/server_decode.rs",
                "crates/game_net/src/control/snapshots_decode.rs",
                "crates/game_net/src/control/codec.rs"
            )
            Primary = $true
        },
        [pscustomobject]@{
            Target = "server_control_event_roundtrip"
            Scope = "crates/game_net/src/control/server_encode.rs, server_decode.rs, snapshots_encode.rs, snapshots_decode.rs, and codec.rs via structure-aware event round trips."
            Description = "Structure-aware round-trip fuzzing for valid server control events and snapshot payloads."
            Paths = @(
                "crates/game_net/src/control/server_encode.rs",
                "crates/game_net/src/control/server_decode.rs",
                "crates/game_net/src/control/snapshots_encode.rs",
                "crates/game_net/src/control/snapshots_decode.rs",
                "crates/game_net/src/control/codec.rs"
            )
            Primary = $false
        },
        [pscustomobject]@{
            Target = "arena_full_snapshot_decode"
            Scope = "crates/game_net/src/control/server_decode.rs and crates/game_net/src/control/snapshots_decode.rs via full arena snapshot decode."
            Description = "Full authoritative arena snapshot decode and validation."
            Paths = @(
                "crates/game_net/src/control/server_decode.rs",
                "crates/game_net/src/control/snapshots_decode.rs",
                "crates/game_net/src/control/codec.rs"
            )
            Primary = $true
        },
        [pscustomobject]@{
            Target = "arena_full_snapshot_roundtrip"
            Scope = "crates/game_net/src/control/server_encode.rs, server_decode.rs, snapshots_encode.rs, snapshots_decode.rs, and codec.rs via structured full snapshot round trips."
            Description = "Structure-aware round-trip fuzzing for valid full arena snapshots."
            Paths = @(
                "crates/game_net/src/control/server_encode.rs",
                "crates/game_net/src/control/server_decode.rs",
                "crates/game_net/src/control/snapshots_encode.rs",
                "crates/game_net/src/control/snapshots_decode.rs",
                "crates/game_net/src/control/codec.rs"
            )
            Primary = $false
        },
        [pscustomobject]@{
            Target = "arena_delta_snapshot_decode"
            Scope = "crates/game_net/src/control/server_decode.rs and crates/game_net/src/control/snapshots_decode.rs via delta arena snapshot decode."
            Description = "Delta authoritative arena snapshot decode and validation."
            Paths = @(
                "crates/game_net/src/control/server_decode.rs",
                "crates/game_net/src/control/snapshots_decode.rs",
                "crates/game_net/src/control/codec.rs"
            )
            Primary = $true
        },
        [pscustomobject]@{
            Target = "arena_delta_snapshot_roundtrip"
            Scope = "crates/game_net/src/control/server_encode.rs, server_decode.rs, snapshots_encode.rs, snapshots_decode.rs, and codec.rs via structured delta snapshot round trips."
            Description = "Structure-aware round-trip fuzzing for valid delta arena snapshots."
            Paths = @(
                "crates/game_net/src/control/server_encode.rs",
                "crates/game_net/src/control/server_decode.rs",
                "crates/game_net/src/control/snapshots_encode.rs",
                "crates/game_net/src/control/snapshots_decode.rs",
                "crates/game_net/src/control/codec.rs"
            )
            Primary = $false
        },
        [pscustomobject]@{
            Target = "webrtc_signal_message_parse"
            Scope = "crates/game_api/src/webrtc/signaling.rs via websocket signaling JSON validation."
            Description = "WebRTC signaling message decode and validation."
            Paths = @("crates/game_api/src/webrtc/signaling.rs")
            Primary = $true
        },
        [pscustomobject]@{
            Target = "control_command_roundtrip"
            Scope = "crates/game_net/src/control/client.rs and crates/game_net/src/control/codec.rs via structured encode/decode differential fuzzing."
            Description = "Structured round-trip fuzzing for valid control command packets."
            Paths = @(
                "crates/game_net/src/control/client.rs",
                "crates/game_net/src/control/codec.rs"
            )
            Primary = $false
        },
        [pscustomobject]@{
            Target = "input_frame_roundtrip"
            Scope = "crates/game_net/src/input.rs via structured encode/decode differential fuzzing."
            Description = "Structured round-trip fuzzing for valid player input packets."
            Paths = @(
                "crates/game_net/src/input.rs",
                "crates/game_net/src/header.rs",
                "crates/game_net/src/packet_types.rs"
            )
            Primary = $false
        },
        [pscustomobject]@{
            Target = "webrtc_signal_message_roundtrip"
            Scope = "crates/game_api/src/webrtc/signaling.rs via structured JSON round-trip fuzzing."
            Description = "Structured round-trip fuzzing for valid signaling messages."
            Paths = @("crates/game_api/src/webrtc/signaling.rs")
            Primary = $false
        },
        [pscustomobject]@{
            Target = "http_route_classification"
            Scope = "crates/game_api/src/observability.rs plus realtime HTTP route labeling."
            Description = "HTTP route classification for observability and low-cardinality request labeling."
            Paths = @("crates/game_api/src/observability.rs")
            Primary = $false
        },
        [pscustomobject]@{
            Target = "observability_metrics_render"
            Scope = "crates/game_api/src/observability.rs via Prometheus rendering, counters, gauges, and route-labeled request metrics."
            Description = "Observability metric rendering and counter/gauge update flows exposed by the hosted ops surface."
            Paths = @("crates/game_api/src/observability.rs")
            Primary = $false
        },
        [pscustomobject]@{
            Target = "player_record_store_parse"
            Scope = "crates/game_api/src/records.rs via persisted record parsing, validation, and canonicalization."
            Description = "Persisted player-record TSV parsing and canonicalization at the storage boundary."
            Paths = @("crates/game_api/src/records.rs")
            Primary = $false
        },
        [pscustomobject]@{
            Target = "ascii_map_parse"
            Scope = "crates/game_content/src/lib.rs via authored ASCII map parsing and validation."
            Description = "ASCII map parsing for the server-authored arena layout."
            Paths = @("crates/game_content/src/lib.rs")
            Primary = $false
        },
        [pscustomobject]@{
            Target = "skill_yaml_parse"
            Scope = "crates/game_content/src/lib.rs via authored YAML skill parsing and validation."
            Description = "YAML skill parsing for runtime-loaded class and skill definitions."
            Paths = @("crates/game_content/src/lib.rs")
            Primary = $false
        }
    )
}

function Get-FuzzReplaySuites {
    return @(
        [pscustomobject]@{
            Package = "game_net"
            Test = "fuzz_corpus_replay"
            Description = "Replay hostile network packet corpora against packet decode and ingress validation."
        },
        [pscustomobject]@{
            Package = "game_api"
            Test = "observability_fuzz_corpus"
            Description = "Replay HTTP route classification corpora against observability route labeling."
        },
        [pscustomobject]@{
            Package = "game_api"
            Test = "records_fuzz_corpus"
            Description = "Replay persisted record-store corpora against TSV parsing and canonicalization."
        },
        [pscustomobject]@{
            Package = "game_content"
            Test = "fuzz_corpus_replay"
            Description = "Replay authored content corpora against ASCII map parsing and YAML skill validation."
        }
    )
}

function Get-Sha256Hex {
    param(
        [string]$Path
    )

    $stream = [System.IO.File]::OpenRead($Path)
    try {
        $sha256 = [System.Security.Cryptography.SHA256]::Create()
        try {
            $hashBytes = $sha256.ComputeHash($stream)
        }
        finally {
            $sha256.Dispose()
        }
    }
    finally {
        $stream.Dispose()
    }

    return -join ($hashBytes | ForEach-Object { $_.ToString("x2") })
}

function Get-FuzzCorpusStats {
    param(
        [string]$ServerRoot,
        [string]$Target
    )

    $seedDir = Join-Path $ServerRoot ("target\fuzz-seed-corpus\" + $Target)
    $generatedDir = Join-Path $ServerRoot ("target\fuzz-generated-corpus\" + $Target)
    $seedFiles = if (Test-Path $seedDir) {
        @(Get-ChildItem -Path $seedDir -File | Sort-Object Name)
    }
    else {
        @()
    }
    $generatedFiles = if (Test-Path $generatedDir) {
        @(Get-ChildItem -Path $generatedDir -File | Sort-Object Name)
    }
    else {
        @()
    }

    $seedHashes = @{}
    foreach ($seedFile in $seedFiles) {
        $seedHashes[(Get-Sha256Hex -Path $seedFile.FullName)] = $true
    }

    $discoveredFiles = @(
        foreach ($generatedFile in $generatedFiles) {
            $hash = Get-Sha256Hex -Path $generatedFile.FullName
            if (-not $seedHashes.ContainsKey($hash)) {
                $generatedFile
            }
        }
    )

    return [pscustomobject]@{
        SeedDir = $seedDir
        GeneratedDir = $generatedDir
        SeedFiles = @($seedFiles)
        GeneratedFiles = @($generatedFiles)
        DiscoveredFiles = @($discoveredFiles)
    }
}

function Get-FuzzArtifactFindings {
    param(
        [string]$ArtifactRoot,
        [object[]]$TargetCatalog
    )

    $targetCatalogByName = @{}
    foreach ($targetInfo in @($TargetCatalog)) {
        $targetCatalogByName[$targetInfo.Target] = $targetInfo
    }

    if (-not (Test-Path $ArtifactRoot)) {
        return @()
    }

    return @(
        Get-ChildItem -Path $ArtifactRoot -Recurse -File |
            Sort-Object FullName |
            ForEach-Object {
                $targetName = Split-Path -Parent $_.FullName | Split-Path -Leaf
                $targetInfo = if ($targetCatalogByName.ContainsKey($targetName)) { $targetCatalogByName[$targetName] } else { $null }
                $bytes = [System.IO.File]::ReadAllBytes($_.FullName)
                $previewLength = [Math]::Min($bytes.Length, 32)
                $previewBytes = if ($previewLength -gt 0) {
                    ($bytes[0..($previewLength - 1)] | ForEach-Object { $_.ToString("X2") }) -join " "
                }
                else {
                    ""
                }
                $asciiPreview = if ($previewLength -gt 0) {
                    -join ($bytes[0..($previewLength - 1)] | ForEach-Object {
                        if ($_ -ge 32 -and $_ -le 126) { [char]$_ } else { "." }
                    })
                }
                else {
                    ""
                }
                $sha256 = Get-Sha256Hex -Path $_.FullName

                [pscustomobject]@{
                    Target = $targetName
                    FileName = $_.Name
                    RelativePath = Convert-ToDisplayPath -Path $_.FullName
                    Size = $_.Length
                    LastWriteTime = $_.LastWriteTimeUtc
                    ReproCommand = "cd server; rustup run nightly cargo fuzz run $targetName fuzz/artifacts/$targetName/$($_.Name)"
                    Scope = if ($null -ne $targetInfo) { $targetInfo.Scope } else { "Unknown scope" }
                    Description = if ($null -ne $targetInfo) { $targetInfo.Description } else { "Unknown target scope." }
                    Sha256 = $sha256
                    HexPreview = $previewBytes
                    AsciiPreview = $asciiPreview
                }
            }
    )
}

function Invoke-CoverageReport {
    param(
        [hashtable]$SourceInventory
    )

    $notes = [System.Collections.Generic.List[string]]::new()
    $summaryPath = Join-Path $coverageRoot "summary.json"
    $reportPath = Join-Path $coverageRoot "index.html"
    $outputPath = Join-Path $coverageRoot "output.html"
    $detailIndex = Join-Path $coverageRoot "html\index.html"

    if (-not (Test-ToolAvailable -CommandName "cargo-llvm-cov")) {
        $notes.Add("Coverage report was skipped because cargo-llvm-cov is not installed.")
        $body = @"
<h1>Coverage Report Unavailable</h1>
<div class="panel">
  <p>cargo-llvm-cov is not installed, so no coverage report could be generated.</p>
  <p class="muted">Install it with <code>./scripts/install-tools.ps1</code> or <code>rustup run stable cargo install --locked cargo-llvm-cov</code>.</p>
</div>
"@
        Write-ReportHtml -Path $reportPath -Title "Coverage Report Unavailable" -Body $body
        Write-ReportHtml -Path $outputPath -Title "Coverage Report Unavailable" -Body $body
        return [pscustomobject]@{
            Name = "Coverage"
            Status = "failed"
            Notes = @($notes)
            IndexPath = "coverage/index.html"
            ErrorMessage = "cargo-llvm-cov is not installed."
        }
    }

    $usedNextest = Test-ToolAvailable -CommandName "cargo-nextest"
    $executionMode = "cargo llvm-cov test --branch"
    if (-not $usedNextest) {
        $notes.Add("cargo-nextest is not installed; coverage fell back to cargo test.")
    }

    try {
        Invoke-CheckedCommand -Description "cargo llvm-cov clean" -Command {
            rustup run $nightlyToolchain cargo llvm-cov clean --workspace | Out-Host
        }
        if (Test-Path $coverageRoot) {
            Remove-Item -Recurse -Force -Path $coverageRoot
        }

        New-Item -ItemType Directory -Force -Path $coverageRoot | Out-Null

        if ($usedNextest) {
            try {
                Invoke-CheckedCommand -Description "cargo llvm-cov nextest" -Command {
                    $env:RARENA_SKIP_LIVE_PROBE_TESTS = "1"
                    try {
                        rustup run $nightlyToolchain cargo llvm-cov nextest --workspace --all-features --branch --no-report | Out-Host
                    }
                    finally {
                        Remove-Item Env:RARENA_SKIP_LIVE_PROBE_TESTS -ErrorAction SilentlyContinue
                    }
                }
                $executionMode = "cargo llvm-cov nextest --branch"
            }
            catch {
                $notes.Add("cargo llvm-cov nextest failed on this host; coverage fell back to cargo llvm-cov test.")
                $notes.Add("nextest failure detail: $($_.Exception.Message)")
                $usedNextest = $false
                Invoke-CheckedCommand -Description "cargo llvm-cov test" -Command {
                    $env:RARENA_SKIP_LIVE_PROBE_TESTS = "1"
                    try {
                        rustup run $nightlyToolchain cargo llvm-cov --workspace --all-features --branch --no-report | Out-Host
                    }
                    finally {
                        Remove-Item Env:RARENA_SKIP_LIVE_PROBE_TESTS -ErrorAction SilentlyContinue
                    }
                }
            }
        }
        else {
            Invoke-CheckedCommand -Description "cargo llvm-cov test" -Command {
                $env:RARENA_SKIP_LIVE_PROBE_TESTS = "1"
                try {
                    rustup run $nightlyToolchain cargo llvm-cov --workspace --all-features --branch --no-report | Out-Host
                }
                finally {
                    Remove-Item Env:RARENA_SKIP_LIVE_PROBE_TESTS -ErrorAction SilentlyContinue
                }
            }
        }

        Invoke-CheckedCommand -Description "cargo llvm-cov json report" -Command {
            rustup run $nightlyToolchain cargo llvm-cov report --branch --json --output-path $summaryPath | Out-Host
        }
        Invoke-CheckedCommand -Description "cargo llvm-cov html report" -Command {
            rustup run $nightlyToolchain cargo llvm-cov report --branch --html --output-dir $coverageRoot | Out-Host
        }

        $coverageJson = Get-Content -Path $summaryPath -Raw | ConvertFrom-Json
        $coverageData = $coverageJson.data | Select-Object -First 1
        $files = @()
        $coveredPaths = @{}

        foreach ($file in @($coverageData.files)) {
            $displayPath = Convert-ToDisplayPath -Path $file.filename
            $coveredPaths[$displayPath] = $true
            $effectiveRegionMetrics = Get-CoverageEffectiveRegionMetrics -CoverageFile $file
            $files += [pscustomobject]@{
                DisplayPath = $displayPath
                NormalizedPath = $displayPath -replace '\\', '/'
                LinePercent = [double]$file.summary.lines.percent
                FunctionPercent = [double]$file.summary.functions.percent
                RegionPercent = [double]$effectiveRegionMetrics.EffectiveRegionPercent
                RawRegionPercent = [double]$effectiveRegionMetrics.RawRegionPercent
                CoveredLines = [int]$file.summary.lines.covered
                TotalLines = [int]$file.summary.lines.count
                CoveredFunctions = [int]$file.summary.functions.covered
                TotalFunctions = [int]$file.summary.functions.count
                CoveredRegions = [int]$effectiveRegionMetrics.EffectiveRegionCovered
                TotalRegions = [int]$effectiveRegionMetrics.EffectiveRegionTotal
                RawCoveredRegions = [int]$effectiveRegionMetrics.RawRegionCovered
                RawTotalRegions = [int]$effectiveRegionMetrics.RawRegionTotal
                BranchOutcomeCovered = [int]$effectiveRegionMetrics.BranchOutcomeCovered
                BranchOutcomeTotal = [int]$effectiveRegionMetrics.BranchOutcomeTotal
            }
        }

        $files = @($files | Sort-Object LinePercent, DisplayPath)
        $runtimeFiles = @(
            $files | Where-Object {
                $SourceInventory.ContainsKey($_.DisplayPath) -and $SourceInventory[$_.DisplayPath].IsRuntimeSource
            }
        )
        $runtimeCoveredLines = ($runtimeFiles | Measure-Object -Property CoveredLines -Sum).Sum
        $runtimeTotalLines = ($runtimeFiles | Measure-Object -Property TotalLines -Sum).Sum
        $runtimeCoveredFunctions = ($runtimeFiles | Measure-Object -Property CoveredFunctions -Sum).Sum
        $runtimeTotalFunctions = ($runtimeFiles | Measure-Object -Property TotalFunctions -Sum).Sum
        $runtimeLinePercent = if ($runtimeTotalLines -gt 0) {
            ([double]$runtimeCoveredLines / [double]$runtimeTotalLines) * 100.0
        }
        else {
            0.0
        }
        $runtimeFunctionPercent = if ($runtimeTotalFunctions -gt 0) {
            ([double]$runtimeCoveredFunctions / [double]$runtimeTotalFunctions) * 100.0
        }
        else {
            0.0
        }
        $runtimeRegionCovered = ($runtimeFiles | Measure-Object -Property CoveredRegions -Sum).Sum
        $runtimeRegionTotal = ($runtimeFiles | Measure-Object -Property TotalRegions -Sum).Sum
        $runtimeRegionPercent = if ($runtimeRegionTotal -gt 0) {
            ([double]$runtimeRegionCovered / [double]$runtimeRegionTotal) * 100.0
        }
        else {
            0.0
        }
        $runtimeRawRegionCovered = ($runtimeFiles | Measure-Object -Property RawCoveredRegions -Sum).Sum
        $runtimeRawRegionTotal = ($runtimeFiles | Measure-Object -Property RawTotalRegions -Sum).Sum
        $runtimeRawRegionPercent = if ($runtimeRawRegionTotal -gt 0) {
            ([double]$runtimeRawRegionCovered / [double]$runtimeRawRegionTotal) * 100.0
        }
        else {
            0.0
        }
        $scoreSummary = New-ScoreSummary `
            -Score (($runtimeLinePercent * 0.05) + ($runtimeFunctionPercent * 0.05) + ($runtimeRegionPercent * 0.90)) `
            -Formula "5% runtime line + 5% runtime function + 90% runtime positive region coverage" `
            -Breakdown @(
                "Runtime lines: $(Format-Percent -Value $runtimeLinePercent)",
                "Runtime functions: $(Format-Percent -Value $runtimeFunctionPercent)",
                "Runtime positive regions: $(Format-Percent -Value $runtimeRegionPercent)"
            )
        $notes.Add("Doctests are validated separately by ./scripts/quality.ps1 doc but are not included here because stable doctest coverage is still unavailable in this workflow.")
        $notes.Add("Headline coverage scoring is scoped to backend runtime source files under crates/*/src/*.rs.")
        $notes.Add("Conditional runtime regions now receive fractional credit from nightly LLVM branch coverage. A partially exercised conditional can score below 100% instead of being treated as fully covered after a single hit.")
        $notes.Add("Runtime positive region coverage excludes benches because only backend runtime source files under crates/*/src/*.rs are scored.")
        $notes.Add("This Rust coverage export does not measure GDScript. The Godot shell exists and has a headless protocol-check script, but browser and WebRTC coverage remain outside this report.")

        foreach ($sourceFile in ($SourceInventory.Keys | Sort-Object)) {
            if ($coveredPaths.ContainsKey($sourceFile)) {
                continue
            }

            $source = $SourceInventory[$sourceFile]
            if ($source.IsPlaceholder) {
                $notes.Add("${sourceFile}: $($source.Reason)")
                continue
            }

            $notes.Add("${sourceFile}: This file did not appear in the coverage export. It may not have been compiled by the covered test targets yet.")
        }

        $rows = foreach ($file in $files) {
            @"
<tr>
  <td><code>$(Escape-Html $file.DisplayPath)</code></td>
  <td>$(Format-Percent -Value $file.LinePercent)</td>
  <td>$($file.CoveredLines) / $($file.TotalLines)</td>
  <td>$(Format-Percent -Value $file.FunctionPercent)</td>
  <td>$($file.CoveredFunctions) / $($file.TotalFunctions)</td>
  <td>$(Format-Percent -Value $file.RegionPercent)</td>
</tr>
"@
        }

        $noteItems = foreach ($note in ($notes | Sort-Object -Unique)) {
            "<li>$(Escape-Html $note)</li>"
        }

        $body = @"
<h1>Coverage Report</h1>
<p class="muted">Commit <code>$(Escape-Html (Get-GitValue -CommandArgs @("rev-parse", "--short", "HEAD") -Fallback "unknown"))</code>. Detailed line-by-line report: <a href="./html/index.html">coverage/html/index.html</a>.</p>
<div class="grid">
  <div class="metric"><span class="muted">Coverage score</span><strong>$(Format-Score -Score $scoreSummary.Score) $(Format-GradeBadge -Grade $scoreSummary.Grade)</strong><div class="detail">$(Escape-Html $scoreSummary.Formula)</div></div>
  <div class="metric"><span class="muted">Runtime line coverage</span><strong>$(Format-Percent -Value $runtimeLinePercent)</strong></div>
  <div class="metric"><span class="muted">Runtime function coverage</span><strong>$(Format-Percent -Value $runtimeFunctionPercent)</strong></div>
  <div class="metric"><span class="muted">Runtime positive region coverage</span><strong>$(Format-Percent -Value $runtimeRegionPercent)</strong><div class="detail">Raw LLVM region coverage: $(Escape-Html (Format-Percent -Value $runtimeRawRegionPercent))</div></div>
  <div class="metric"><span class="muted">Scored runtime files</span><strong>$($runtimeFiles.Count)</strong></div>
  <div class="metric"><span class="muted">Execution mode</span><strong>$(Escape-Html $executionMode)</strong></div>
</div>
<div class="panel">
  <p class="muted">The headline score is based on backend runtime source files. Conditional regions expand into branch outcomes so partially tested decisions receive partial credit instead of binary full credit. The table below still includes tooling and test files emitted by the coverage export.</p>
  <h2>Per-file summary</h2>
  <table>
    <thead>
      <tr>
        <th>File</th>
        <th>Lines</th>
        <th>Covered lines</th>
        <th>Functions</th>
        <th>Covered functions</th>
        <th>Positive regions</th>
      </tr>
    </thead>
    <tbody>
$(($rows -join "`n"))
    </tbody>
  </table>
</div>
<div class="panel">
  <h2>Current testing gaps</h2>
  <ul>
$(($noteItems -join "`n"))
  </ul>
</div>
<p class="footer"><a href="../index.html">Back to report index</a></p>
"@

        Write-ReportHtml -Path $reportPath -Title "Coverage Report" -Body $body
        Write-ReportHtml -Path $outputPath -Title "Coverage Report" -Body $body

        return [pscustomobject]@{
            Name = "Coverage"
            Status = "ok"
            Notes = @($notes | Sort-Object -Unique)
            IndexPath = "coverage/index.html"
            ErrorMessage = $null
            ScoreSummary = $scoreSummary
            Summary = [pscustomobject]@{
                Lines = $runtimeLinePercent
                Functions = $runtimeFunctionPercent
                Regions = $runtimeRegionPercent
                RawRegions = $runtimeRawRegionPercent
            }
        }
    }
    catch {
        $errorMessage = $_.Exception.Message
        $notes.Add("Coverage report generation failed: $errorMessage")
        $body = @"
<h1>Coverage Report Failed</h1>
<div class="panel">
  <p>The coverage step could not complete.</p>
  <p><code>$(Escape-Html $errorMessage)</code></p>
</div>
<p class="footer"><a href="../index.html">Back to report index</a></p>
"@
        Write-ReportHtml -Path $reportPath -Title "Coverage Report Failed" -Body $body
        Write-ReportHtml -Path $outputPath -Title "Coverage Report Failed" -Body $body

        return [pscustomobject]@{
            Name = "Coverage"
            Status = "failed"
            Notes = @($notes)
            IndexPath = "coverage/index.html"
            ErrorMessage = $errorMessage
        }
    }
}

function Invoke-ComplexityReport {
    param(
        [hashtable]$SourceInventory
    )

    $notes = [System.Collections.Generic.List[string]]::new()
    $dataRoot = Join-Path $complexityRoot "data"
    $reportPath = Join-Path $complexityRoot "index.html"
    $outputPath = Join-Path $complexityRoot "output.html"
    $summaryPath = Join-Path $complexityRoot "summary.json"

    if (-not (Test-ToolAvailable -CommandName "rust-code-analysis-cli")) {
        $notes.Add("Complexity report was skipped because rust-code-analysis-cli is not installed.")
        $body = @"
<h1>Complexity Report Unavailable</h1>
<div class="panel">
  <p>rust-code-analysis-cli is not installed, so no complexity report could be generated.</p>
  <p class="muted">Install it with <code>./scripts/install-tools.ps1</code> or <code>rustup run stable cargo install --locked rust-code-analysis-cli</code>.</p>
</div>
"@
        Write-ReportHtml -Path $reportPath -Title "Complexity Report Unavailable" -Body $body
        Write-ReportHtml -Path $outputPath -Title "Complexity Report Unavailable" -Body $body
        return [pscustomobject]@{
            Name = "Complexity"
            Status = "failed"
            Notes = @($notes)
            IndexPath = "complexity/index.html"
            ErrorMessage = "rust-code-analysis-cli is not installed."
        }
    }

    try {
        if (Test-Path $complexityRoot) {
            Remove-Item -Recurse -Force -Path $complexityRoot
        }

        $crateDataRoot = Join-Path $dataRoot "crates"
        $binDataRoot = Join-Path $dataRoot "bin"
        New-Item -ItemType Directory -Force -Path $crateDataRoot | Out-Null
        New-Item -ItemType Directory -Force -Path $binDataRoot | Out-Null

        Invoke-CheckedCommand -Description "rust-code-analysis crates" -Command {
            rust-code-analysis-cli --metrics --output-format json --output $crateDataRoot --paths crates | Out-Host
        }
        Invoke-CheckedCommand -Description "rust-code-analysis bin" -Command {
            rust-code-analysis-cli --metrics --output-format json --output $binDataRoot --paths bin | Out-Host
        }

        $fileMetrics = @()
        $functionMetrics = @()
        $analyzedPaths = @{}

        foreach ($jsonPath in Get-ChildItem -Path $dataRoot -Recurse -File -Filter *.json) {
            $jsonText = Get-Content -Path $jsonPath.FullName -Raw
            $jsonText = $jsonText.Replace('"N1":', '"N1_upper":').Replace('"N2":', '"N2_upper":')
            $metrics = $jsonText | ConvertFrom-Json
            $displayPath = Convert-ToDisplayPath -Path $metrics.name
            $analyzedPaths[$displayPath] = $true

            $fileMetrics += [pscustomobject]@{
                DisplayPath = $displayPath
                Mi = [double]$metrics.metrics.mi.mi_visual_studio
                MiGrade = Get-MaintainabilityGrade -Score ([double]$metrics.metrics.mi.mi_visual_studio)
                Cyclomatic = [double]$metrics.metrics.cyclomatic.sum
                Cognitive = [double]$metrics.metrics.cognitive.sum
                FunctionCount = [int]$metrics.metrics.nom.functions
                Sloc = [double]$metrics.metrics.loc.sloc
            }

            foreach ($node in @($metrics.spaces)) {
                $functionMetrics += Get-ComplexityFunctions -Node $node -FilePath $displayPath
            }
        }

        foreach ($function in $functionMetrics) {
            Add-Member -InputObject $function -NotePropertyName CyclomaticGrade -NotePropertyValue (Get-CyclomaticGrade -Score $function.Cyclomatic)
            Add-Member -InputObject $function -NotePropertyName MiGrade -NotePropertyValue (Get-MaintainabilityGrade -Score $function.Mi)
        }

        $functionMetrics = @($functionMetrics | Sort-Object @{ Expression = "Cyclomatic"; Descending = $true }, @{ Expression = "Cognitive"; Descending = $true }, FilePath, Name)

        $fileFunctionSummaries = @{}
        foreach ($group in ($functionMetrics | Group-Object FilePath)) {
            $worstFunction = $group.Group | Sort-Object @{ Expression = "Cyclomatic"; Descending = $true }, @{ Expression = "Cognitive"; Descending = $true }, Name | Select-Object -First 1
            $averageCyclomatic = [double](($group.Group | Measure-Object -Property Cyclomatic -Average).Average)
            $fileFunctionSummaries[$group.Name] = [pscustomobject]@{
                WorstCyclomatic = [double]$worstFunction.Cyclomatic
                WorstGrade = [string]$worstFunction.CyclomaticGrade
                AverageCyclomatic = $averageCyclomatic
                AverageGrade = Get-CyclomaticGrade -Score $averageCyclomatic
            }
        }

        foreach ($file in $fileMetrics) {
            $summary = $null
            if ($fileFunctionSummaries.ContainsKey($file.DisplayPath)) {
                $summary = $fileFunctionSummaries[$file.DisplayPath]
            }

            Add-Member -InputObject $file -NotePropertyName WorstFunctionCyclomatic -NotePropertyValue $(if ($null -ne $summary) { $summary.WorstCyclomatic } else { $null })
            Add-Member -InputObject $file -NotePropertyName WorstFunctionGrade -NotePropertyValue $(if ($null -ne $summary) { $summary.WorstGrade } else { $null })
            Add-Member -InputObject $file -NotePropertyName AverageFunctionCyclomatic -NotePropertyValue $(if ($null -ne $summary) { $summary.AverageCyclomatic } else { $null })
            Add-Member -InputObject $file -NotePropertyName AverageFunctionGrade -NotePropertyValue $(if ($null -ne $summary) { $summary.AverageGrade } else { $null })
        }

        $scoredPathSet = @{}
        foreach ($source in @($SourceInventory.Values | Where-Object { $_.IsScoredCodeSource })) {
            $scoredPathSet[$source.DisplayPath] = $true
        }

        $scoredFileMetrics = @(
            $fileMetrics |
                Where-Object { $scoredPathSet.ContainsKey($_.DisplayPath) } |
                Sort-Object @{ Expression = { Get-GradeSeverity -Grade $_.WorstFunctionGrade }; Descending = $true }, @{ Expression = { Get-GradeSeverity -Grade $_.AverageFunctionGrade }; Descending = $true }, DisplayPath
        )
        $scoredFunctionMetrics = @(
            $functionMetrics |
                Where-Object { $scoredPathSet.ContainsKey($_.FilePath) } |
                Sort-Object @{ Expression = "Cyclomatic"; Descending = $true }, @{ Expression = "Cognitive"; Descending = $true }, FilePath, Name
        )
        $supplementalFileMetrics = @(
            $fileMetrics |
                Where-Object { -not $scoredPathSet.ContainsKey($_.DisplayPath) } |
                Sort-Object @{ Expression = { Get-GradeSeverity -Grade $_.WorstFunctionGrade }; Descending = $true }, @{ Expression = { Get-GradeSeverity -Grade $_.AverageFunctionGrade }; Descending = $true }, DisplayPath
        )

        $filesWithFunctions = @($scoredFileMetrics | Where-Object { $null -ne $_.AverageFunctionGrade })
        $averageWorstFunctionScore = if ($filesWithFunctions.Count -gt 0) {
            [double](($filesWithFunctions | ForEach-Object { Get-CyclomaticGradeScore -Grade $_.WorstFunctionGrade } | Measure-Object -Average).Average)
        }
        else {
            0.0
        }
        $averageFunctionScore = if ($filesWithFunctions.Count -gt 0) {
            [double](($filesWithFunctions | ForEach-Object { Get-CyclomaticGradeScore -Grade $_.AverageFunctionGrade } | Measure-Object -Average).Average)
        }
        else {
            0.0
        }
        $manageableFiles = @($filesWithFunctions | Where-Object { $_.WorstFunctionGrade -notin @("E", "F") })
        $manageableCodePercent = if ($filesWithFunctions.Count -eq 0) {
            0.0
        }
        else {
            ($manageableFiles.Count / $filesWithFunctions.Count) * 100.0
        }
        $scoreSummary = New-ScoreSummary `
            -Score (($averageWorstFunctionScore * 0.5) + ($averageFunctionScore * 0.3) + ($manageableCodePercent * 0.2)) `
            -Formula "50% average non-test code worst-function grade + 30% average non-test code per-file function grade + 20% non-test code files without E/F hotspots" `
            -Breakdown @(
                "Average non-test code worst-function grade score: $("{0:N2}" -f $averageWorstFunctionScore)",
                "Average non-test code function grade score: $("{0:N2}" -f $averageFunctionScore)",
                "Non-test code files without E/F hotspots: $(Format-Percent -Value $manageableCodePercent)"
            )

        $notes.Add("Headline complexity scoring is scoped to all non-test Rust code under crates/ and bin/, with fuzzing code excluded.")
        $notes.Add("Tests stay visible in the supplemental table but do not affect the headline complexity score.")
        $notes.Add("File-level maintainability index is still shown for context, but the headline score is driven by function-grade health because rust-code-analysis file MI is unstable on several larger Rust modules in this repo.")
        $notes.Add("Placeholder crates with only crate-level docs and attributes have no meaningful complexity data yet.")
        $notes.Add("Cyclomatic grades use Xenon/Radon bands: A 1-5, B 6-10, C 11-20, D 21-30, E 31-40, F 41+.")
        $notes.Add("Maintainability grades use Radon MI bands: A >19, B 10-19, C <=9.")

        foreach ($sourceFile in ($SourceInventory.Keys | Sort-Object)) {
            if ($analyzedPaths.ContainsKey($sourceFile)) {
                continue
            }

            $source = $SourceInventory[$sourceFile]
            if ($source.IsPlaceholder) {
                $notes.Add("${sourceFile}: $($source.Reason)")
            }
            else {
                $notes.Add("${sourceFile}: This file was not included in the complexity export.")
            }
        }

        $fileRows = foreach ($file in $scoredFileMetrics) {
            @"
<tr>
  <td><code>$(Escape-Html $file.DisplayPath)</code></td>
  <td>$("{0:N2}" -f $file.Mi) $(Format-GradeBadge -Grade $file.MiGrade)</td>
  <td>$(if ($null -ne $file.WorstFunctionCyclomatic) { '{0:N2} {1}' -f $file.WorstFunctionCyclomatic, (Format-GradeBadge -Grade $file.WorstFunctionGrade) } else { '<span class="muted">n/a</span>' })</td>
  <td>$(if ($null -ne $file.AverageFunctionCyclomatic) { '{0:N2} {1}' -f $file.AverageFunctionCyclomatic, (Format-GradeBadge -Grade $file.AverageFunctionGrade) } else { '<span class="muted">n/a</span>' })</td>
  <td>$("{0:N0}" -f $file.Cyclomatic)</td>
  <td>$("{0:N0}" -f $file.Cognitive)</td>
  <td>$("{0:N0}" -f $file.FunctionCount)</td>
  <td>$("{0:N0}" -f $file.Sloc)</td>
</tr>
"@
        }

        $supplementalRows = foreach ($file in $supplementalFileMetrics) {
            $sourceInfo = if ($SourceInventory.ContainsKey($file.DisplayPath)) { $SourceInventory[$file.DisplayPath] } else { $null }
            @"
<tr>
  <td><code>$(Escape-Html $file.DisplayPath)</code></td>
  <td>$(Escape-Html (Get-SourceCategoryLabel -SourceInfo $sourceInfo))</td>
  <td>$("{0:N2}" -f $file.Mi) $(Format-GradeBadge -Grade $file.MiGrade)</td>
  <td>$(if ($null -ne $file.WorstFunctionCyclomatic) { '{0:N2} {1}' -f $file.WorstFunctionCyclomatic, (Format-GradeBadge -Grade $file.WorstFunctionGrade) } else { '<span class="muted">n/a</span>' })</td>
  <td>$(if ($null -ne $file.AverageFunctionCyclomatic) { '{0:N2} {1}' -f $file.AverageFunctionCyclomatic, (Format-GradeBadge -Grade $file.AverageFunctionGrade) } else { '<span class="muted">n/a</span>' })</td>
  <td>$("{0:N0}" -f $file.FunctionCount)</td>
</tr>
"@
        }

        $hotspotRows = foreach ($function in ($scoredFunctionMetrics | Select-Object -First 15)) {
            @"
<tr>
  <td><code>$(Escape-Html $function.FilePath)</code></td>
  <td><code>$(Escape-Html $function.Name)</code></td>
  <td>$("{0:N0}" -f $function.Cognitive)</td>
  <td>$("{0:N0}" -f $function.Cyclomatic) $(Format-GradeBadge -Grade $function.CyclomaticGrade)</td>
  <td>$("{0:N2}" -f $function.Mi) $(Format-GradeBadge -Grade $function.MiGrade)</td>
  <td>$($function.StartLine)-$($function.EndLine)</td>
</tr>
"@
        }

        $noteItems = foreach ($note in ($notes | Sort-Object -Unique)) {
            "<li>$(Escape-Html $note)</li>"
        }

        $worstFunction = $scoredFunctionMetrics | Select-Object -First 1
        $worstFile = $scoredFileMetrics | Select-Object -First 1
        $supplementalPanel = if ($supplementalRows.Count -gt 0) {
            @"
<div class="panel">
  <h2>Supplemental unscored files</h2>
  <p class="muted">These files are still analyzed and shown here, but they do not affect the headline complexity score.</p>
  <table>
    <thead>
      <tr>
        <th>File</th>
        <th>Category</th>
        <th>MI (VS)</th>
        <th>Worst fn CC</th>
        <th>Avg fn CC</th>
        <th>Functions</th>
      </tr>
    </thead>
    <tbody>
$(($supplementalRows -join "`n"))
    </tbody>
  </table>
</div>
"@
        }
        else {
            ""
        }

        $body = @"
<h1>Complexity Report</h1>
<p class="muted">Commit <code>$(Escape-Html (Get-GitValue -CommandArgs @("rev-parse", "--short", "HEAD") -Fallback "unknown"))</code>. Raw analyzer output lives under <code>target/reports/complexity/data</code>.</p>
<div class="grid">
  <div class="metric"><span class="muted">Complexity score</span><strong>$(Format-Score -Score $scoreSummary.Score) $(Format-GradeBadge -Grade $scoreSummary.Grade)</strong><div class="detail">$(Escape-Html $scoreSummary.Formula)</div></div>
  <div class="metric"><span class="muted">Scored code files</span><strong>$($scoredFileMetrics.Count)</strong></div>
  <div class="metric"><span class="muted">Supplemental files</span><strong>$($supplementalFileMetrics.Count)</strong></div>
  <div class="metric"><span class="muted">Tracked code functions</span><strong>$($scoredFunctionMetrics.Count)</strong></div>
  <div class="metric"><span class="muted">Worst function CC</span><strong>$(if ($null -ne $worstFunction) { '{0} ({1:N0})' -f $worstFunction.CyclomaticGrade, $worstFunction.Cyclomatic } else { 'n/a' })</strong></div>
  <div class="metric"><span class="muted">Worst file avg CC</span><strong>$(if (($null -ne $worstFile) -and ($null -ne $worstFile.AverageFunctionCyclomatic)) { '{0} ({1:N2})' -f $worstFile.AverageFunctionGrade, $worstFile.AverageFunctionCyclomatic } else { 'n/a' })</strong></div>
  <div class="metric"><span class="muted">Code files without E/F</span><strong>$(Format-Percent -Value $manageableCodePercent)</strong></div>
</div>
<div class="panel">
  <h2>Grade scale</h2>
  <p><strong>Cyclomatic:</strong> A 1-5, B 6-10, C 11-20, D 21-30, E 31-40, F 41+.</p>
  <p><strong>Maintainability:</strong> A &gt;19, B 10-19, C &lt;=9.</p>
  <p class="muted">The headline score is based on non-test code function health, while raw MI stays visible as a supporting signal.</p>
</div>
<div class="panel">
  <h2>Scored non-test code files</h2>
  <table>
    <thead>
      <tr>
        <th>File</th>
        <th>MI (VS)</th>
        <th>Worst fn CC</th>
        <th>Avg fn CC</th>
        <th>Cyclomatic sum</th>
        <th>Cognitive sum</th>
        <th>Functions</th>
        <th>SLOC</th>
      </tr>
    </thead>
    <tbody>
$(($fileRows -join "`n"))
    </tbody>
  </table>
</div>
<div class="panel">
  <h2>Top non-test code function hotspots</h2>
  <table>
    <thead>
      <tr>
        <th>File</th>
        <th>Function</th>
        <th>Cognitive</th>
        <th>Cyclomatic</th>
        <th>MI (VS)</th>
        <th>Lines</th>
      </tr>
    </thead>
    <tbody>
$(($hotspotRows -join "`n"))
    </tbody>
  </table>
</div>
$supplementalPanel
<div class="panel">
  <h2>Current analysis gaps</h2>
  <ul>
$(($noteItems -join "`n"))
  </ul>
</div>
<p class="footer"><a href="../index.html">Back to report index</a></p>
"@

        Write-ReportHtml -Path $reportPath -Title "Complexity Report" -Body $body
        Write-ReportHtml -Path $outputPath -Title "Complexity Report" -Body $body

        $summaryPayload = [pscustomobject]@{
            commit = Get-GitValue -CommandArgs @("rev-parse", "--short", "HEAD") -Fallback "unknown"
            score = [pscustomobject]@{
                value = [math]::Round([double]$scoreSummary.Score, 2)
                grade = $scoreSummary.Grade
                formula = $scoreSummary.Formula
                breakdown = @($scoreSummary.Breakdown)
            }
            code = [pscustomobject]@{
                file_count = $scoredFileMetrics.Count
                supplemental_file_count = $supplementalFileMetrics.Count
                function_count = $scoredFunctionMetrics.Count
                files_without_ef_hotspots_percent = [math]::Round([double]$manageableCodePercent, 2)
            }
            worst = [pscustomobject]@{
                function = if ($null -ne $worstFunction) {
                    [pscustomobject]@{
                        file = $worstFunction.FilePath
                        name = $worstFunction.Name
                        cyclomatic = $worstFunction.Cyclomatic
                        cyclomatic_grade = $worstFunction.CyclomaticGrade
                        cognitive = $worstFunction.Cognitive
                        maintainability_index = $worstFunction.Mi
                        maintainability_grade = $worstFunction.MiGrade
                        start_line = $worstFunction.StartLine
                        end_line = $worstFunction.EndLine
                    }
                }
                else {
                    $null
                }
                file = if ($null -ne $worstFile) {
                    [pscustomobject]@{
                        path = $worstFile.DisplayPath
                        average_function_cyclomatic = $worstFile.AverageFunctionCyclomatic
                        average_function_grade = $worstFile.AverageFunctionGrade
                        worst_function_cyclomatic = $worstFile.WorstFunctionCyclomatic
                        worst_function_grade = $worstFile.WorstFunctionGrade
                        maintainability_index = $worstFile.Mi
                        maintainability_grade = $worstFile.MiGrade
                    }
                }
                else {
                    $null
                }
            }
            hotspots = @(
                $scoredFunctionMetrics |
                    Select-Object -First 10 |
                    ForEach-Object {
                        [pscustomobject]@{
                            file = $_.FilePath
                            name = $_.Name
                            cyclomatic = $_.Cyclomatic
                            cyclomatic_grade = $_.CyclomaticGrade
                            cognitive = $_.Cognitive
                            maintainability_index = $_.Mi
                            maintainability_grade = $_.MiGrade
                            start_line = $_.StartLine
                            end_line = $_.EndLine
                        }
                    }
            )
            notes = @($notes | Sort-Object -Unique)
        }
        $summaryPayload | ConvertTo-Json -Depth 8 | Set-Content -Path $summaryPath -Encoding UTF8

        return [pscustomobject]@{
            Name = "Complexity"
            Status = "ok"
            Notes = @($notes | Sort-Object -Unique)
            IndexPath = "complexity/index.html"
            ErrorMessage = $null
            ScoreSummary = $scoreSummary
        }
    }
    catch {
        $errorMessage = $_.Exception.Message
        $notes.Add("Complexity report generation failed: $errorMessage")
        $body = @"
<h1>Complexity Report Failed</h1>
<div class="panel">
  <p>The complexity step could not complete.</p>
  <p><code>$(Escape-Html $errorMessage)</code></p>
</div>
<p class="footer"><a href="../index.html">Back to report index</a></p>
"@
        Write-ReportHtml -Path $reportPath -Title "Complexity Report Failed" -Body $body
        Write-ReportHtml -Path $outputPath -Title "Complexity Report Failed" -Body $body

        return [pscustomobject]@{
            Name = "Complexity"
            Status = "failed"
            Notes = @($notes)
            IndexPath = "complexity/index.html"
            ErrorMessage = $errorMessage
        }
    }
}

function Invoke-CallgraphReport {
    param(
        [hashtable]$SourceInventory
    )

    $notes = [System.Collections.Generic.List[string]]::new()
    $reportPath = Join-Path $callgraphRoot "index.html"
    $outputPath = Join-Path $callgraphRoot "output.html"
    $fullScipPath = Join-Path $callgraphRoot "index.scip"
    $fullScipJsonPath = Join-Path $callgraphRoot "index.scip.json"
    $scipIndexerStdoutPath = Join-Path $callgraphRoot "rust-analyzer.scip.stdout.txt"
    $scipIndexerDiagnosticsPath = Join-Path $callgraphRoot "rust-analyzer.scip.stderr.txt"
    $coreFallbackSvgPath = Join-Path $callgraphRoot "backend_core.simple.svg"
    $overviewFallbackSvgPath = Join-Path $callgraphRoot "backend_core.overview.simple.svg"
    $summaryJsonPath = Join-Path $callgraphRoot "backend_core.summary.json"
    $backendCoreFiles = @(Get-BackendCoreRuntimeFiles -SourceInventory $SourceInventory)
    $entryFiles = @(
        "crates/game_api/src/realtime.rs"
    )

    if (-not (Test-ToolAvailable -CommandName "rust-analyzer")) {
        $notes.Add("Call graph report was skipped because rust-analyzer is not installed.")
        $body = @"
<h1>Call Graph Unavailable</h1>
<div class="panel">
  <p>rust-analyzer is not installed, so the backend call graph could not be generated.</p>
  <p class="muted">Install it with <code>./scripts/install-tools.ps1 -CallgraphOnly</code>.</p>
</div>
"@
        Write-ReportHtml -Path $reportPath -Title "Call Graph Unavailable" -Body $body
        Write-ReportHtml -Path $outputPath -Title "Call Graph Unavailable" -Body $body
        return [pscustomobject]@{
            Name = "Call Graph"
            Status = "failed"
            Notes = @($notes)
            IndexPath = "callgraph/index.html"
            ErrorMessage = "rust-analyzer is not installed."
        }
    }

    try {
        if (Test-Path $callgraphRoot) {
            Remove-Item -Recurse -Force -Path $callgraphRoot
        }
        New-Item -ItemType Directory -Force -Path $callgraphRoot | Out-Null

        Push-Location $serverRoot
        try {
            if (Test-Path $fullScipPath) {
                Remove-Item -Force -Path $fullScipPath
            }
            if (Test-Path $scipIndexerStdoutPath) {
                Remove-Item -Force -Path $scipIndexerStdoutPath
            }
            if (Test-Path $scipIndexerDiagnosticsPath) {
                Remove-Item -Force -Path $scipIndexerDiagnosticsPath
            }

            $scipIndexerWarning = $null
            $scipIndexerProcess = Start-Process -FilePath "rust-analyzer" `
                -ArgumentList @("scip", ".", "--output", $fullScipPath) `
                -WorkingDirectory $serverRoot `
                -NoNewWindow `
                -Wait `
                -PassThru `
                -RedirectStandardOutput $scipIndexerStdoutPath `
                -RedirectStandardError $scipIndexerDiagnosticsPath
            $scipIndexerExitCode = $scipIndexerProcess.ExitCode
            if (Test-Path $scipIndexerStdoutPath) {
                Get-Content -Path $scipIndexerStdoutPath | Out-Host
                Remove-Item -Force -Path $scipIndexerStdoutPath
            }
            $scipIndexerDiagnostics = @(Get-RustAnalyzerScipDiagnostics -Path $scipIndexerDiagnosticsPath)
            if ($scipIndexerDiagnostics.Count -gt 0) {
                Set-Content -Path $scipIndexerDiagnosticsPath -Value ($scipIndexerDiagnostics -join "`n") -Encoding utf8
            }
            elseif (($scipIndexerExitCode -eq 0) -and (Test-Path $scipIndexerDiagnosticsPath)) {
                Remove-Item -Force -Path $scipIndexerDiagnosticsPath
            }
            if (-not (Test-Path $fullScipPath)) {
                if ($scipIndexerExitCode -ne 0) {
                    throw "rust-analyzer scip failed with exit code $scipIndexerExitCode."
                }

                throw "rust-analyzer did not produce $fullScipPath."
            }
            if (($scipIndexerExitCode -ne 0) -or ($scipIndexerDiagnostics.Count -gt 0)) {
                $duplicateSymbolCount = @(
                    $scipIndexerDiagnostics |
                        Where-Object { $_ -like "*Duplicate symbol:*" }
                ).Count
                if ($duplicateSymbolCount -gt 0) {
                    $scipIndexerWarning = "rust-analyzer scip emitted duplicate-symbol diagnostics for $duplicateSymbolCount symbols; see rust-analyzer.scip.stderr.txt."
                    if ($scipIndexerExitCode -ne 0) {
                        $scipIndexerWarning += " The process exited with code $scipIndexerExitCode."
                    }
                }
                else {
                    $scipIndexerWarning = if ($scipIndexerExitCode -ne 0) {
                        "rust-analyzer scip exited with code $scipIndexerExitCode."
                    }
                    else {
                        "rust-analyzer scip emitted diagnostics."
                    }

                    $diagnosticPreview = Format-DiagnosticPreview -Lines $scipIndexerDiagnostics
                    if (-not [string]::IsNullOrWhiteSpace($diagnosticPreview)) {
                        $scipIndexerWarning = "$scipIndexerWarning $diagnosticPreview"
                    }
                }
            }
        }
        finally {
            Pop-Location
        }

        Invoke-CheckedCommand -Description "scip_json_dump" -Command {
            rustup run stable cargo run -p scip_json_dump --quiet -- $fullScipPath $fullScipJsonPath | Out-Host
        }

        $callgraphArgs = @("run", "-p", "backend_callgraph", "--quiet", "--", $fullScipPath, $callgraphRoot)
        foreach ($backendCoreFile in $backendCoreFiles) {
            $callgraphArgs += @("--backend-file", $backendCoreFile)
        }
        foreach ($entryFile in $entryFiles) {
            $callgraphArgs += @("--entry-file", $entryFile)
        }

        Invoke-CheckedCommand -Description "backend_callgraph" -Command {
            & rustup run stable cargo @callgraphArgs | Out-Host
        }

        if (-not (Test-Path $summaryJsonPath)) {
            throw "backend call graph generation did not produce backend_core.summary.json."
        }

        $summary = Get-Content -Path $summaryJsonPath -Raw | ConvertFrom-Json
        $documents = @((Get-Content -Path $fullScipJsonPath -Raw | ConvertFrom-Json).documents)

        $documentCount = $documents.Count
        $symbolCount = @($documents | ForEach-Object { @($_.symbols).Count } | Measure-Object -Sum).Sum
        $occurrenceCount = @($documents | ForEach-Object { @($_.occurrences).Count } | Measure-Object -Sum).Sum
        $overviewEmbedAsset = if (Test-Path $overviewFallbackSvgPath) {
            "backend_core.overview.simple.svg"
        }
        else {
            $null
        }

        $notes.Add("The backend call graph is now local-only: tests, enum members, bodyless declarations, and external dependency nodes are excluded from the rendered graphs.")
        $notes.Add("The overview graph groups the backend by source file, while the detailed graph retains function-level edges for debugging.")
        $notes.Add("The curated graphs root from runtime entry files in game_api and keep only functions reachable from those entrypoints.")
        if ($summary.omitted_unreachable_nodes -gt 0) {
            $notes.Add("$($summary.omitted_unreachable_nodes) local helper functions were omitted because they are not reachable from the selected runtime entrypoints.")
        }
        if ($summary.omitted_test_nodes -gt 0) {
            $notes.Add("$($summary.omitted_test_nodes) test functions were omitted from the rendered graph.")
        }
        if ($summary.omitted_bodyless_nodes -gt 0) {
            $notes.Add("$($summary.omitted_bodyless_nodes) callable-looking definitions were omitted because no executable body could be located.")
        }
        if (-not [string]::IsNullOrWhiteSpace($scipIndexerWarning)) {
            $notes.Add("rust-analyzer scip reported upstream indexing errors but still produced a usable SCIP index; the curated callgraph artifacts were generated from that output.")
            $notes.Add("Indexer diagnostics were captured in rust-analyzer.scip.stderr.txt so CI keeps upstream rust-analyzer warnings out of GitHub file annotations.")
            $notes.Add("Indexer warning: $scipIndexerWarning")
        }

        $embedBlock = if ($null -ne $overviewEmbedAsset) {
            @"
<div class="panel">
  <h2>Main Backend Overview</h2>
  <p class="muted">Condensed view grouped by backend source file. Use the detailed function graph from the artifact list when you need exact per-function edges.</p>
  <object data="./$(Escape-Html $overviewEmbedAsset)" type="image/svg+xml" style="width: 100%; min-height: 32rem; border: 1px solid var(--line); border-radius: 12px;">
    <p>SVG preview could not be embedded. Open <a href="./$(Escape-Html $overviewEmbedAsset)">the overview SVG directly</a>.</p>
  </object>
</div>
"@
        }
        else {
            @"
<div class="panel">
  <h2>Preview Unavailable</h2>
  <p>No curated backend overview SVG preview could be generated.</p>
</div>
"@
        }

        $artifactItems = @(
            '<li><a href="./index.scip">Raw SCIP index</a></li>',
            '<li><a href="./index.scip.json">SCIP JSON</a></li>',
            '<li><a href="./backend_core.summary.json">Curated backend summary JSON</a></li>',
            '<li><a href="./backend_core.overview.dot">Backend overview DOT</a></li>',
            '<li><a href="./backend_core.overview.simple.svg">Backend overview SVG</a></li>',
            '<li><a href="./backend_core.dot">Curated backend core DOT</a></li>'
        )
        if (Test-Path $scipIndexerDiagnosticsPath) {
            $artifactItems += '<li><a href="./rust-analyzer.scip.stderr.txt">rust-analyzer SCIP diagnostics</a></li>'
        }
        if (Test-Path $coreFallbackSvgPath) {
            $artifactItems += '<li><a href="./backend_core.simple.svg">Curated backend core simple SVG</a></li>'
        }

        $noteItems = foreach ($note in ($notes | Sort-Object -Unique)) {
            "<li>$(Escape-Html $note)</li>"
        }

        $rootItems = foreach ($root in @($summary.roots)) {
            "<li><code>$(Escape-Html ([string]$root))</code></li>"
        }

        $fanOutRows = foreach ($item in @($summary.top_fan_out)) {
            @"
<tr>
  <td><code>$(Escape-Html ([string]$item.label))</code></td>
  <td><code>$(Escape-Html ([string]$item.file))</code></td>
  <td>$([int]$item.count)</td>
</tr>
"@
        }

        $fanInRows = foreach ($item in @($summary.top_fan_in)) {
            @"
<tr>
  <td><code>$(Escape-Html ([string]$item.label))</code></td>
  <td><code>$(Escape-Html ([string]$item.file))</code></td>
  <td>$([int]$item.count)</td>
</tr>
"@
        }

        $externalItems = foreach ($item in @($summary.hidden_external_references)) {
            "<li><code>$(Escape-Html ([string]$item.crate_name))</code>: $([int]$item.count) hidden call-site references</li>"
        }

        $body = @"
<h1>Call Graph Report</h1>
<p class="muted">Commit <code>$(Escape-Html (Get-GitValue -CommandArgs @("rev-parse", "--short", "HEAD") -Fallback "unknown"))</code>. Generated from <code>rust-analyzer scip .</code> plus the repo-local <code>backend_callgraph</code> filter.</p>
<div class="grid">
  <div class="metric"><span class="muted">SCIP documents</span><strong>$documentCount</strong></div>
  <div class="metric"><span class="muted">Symbol definitions</span><strong>$symbolCount</strong></div>
  <div class="metric"><span class="muted">Occurrences</span><strong>$occurrenceCount</strong></div>
  <div class="metric"><span class="muted">Overview files</span><strong>$([int]$summary.overview_file_count)</strong></div>
  <div class="metric"><span class="muted">Overview edges</span><strong>$([int]$summary.overview_edge_count)</strong></div>
  <div class="metric"><span class="muted">Rendered nodes</span><strong>$([int]$summary.node_count)</strong></div>
  <div class="metric"><span class="muted">Rendered edges</span><strong>$([int]$summary.edge_count)</strong></div>
  <div class="metric"><span class="muted">Entry roots</span><strong>$([int]$summary.root_count)</strong></div>
  <div class="metric"><span class="muted">Renderer</span><strong>Repo-local DOT + safe SVG</strong></div>
</div>
$embedBlock
<div class="panel">
  <h2>Artifacts</h2>
  <ul>
$(($artifactItems -join "`n"))
  </ul>
</div>
<div class="panel">
  <h2>Selected Runtime Roots</h2>
  <ul>
$(($rootItems -join "`n"))
  </ul>
</div>
<div class="panel">
  <h2>Functions Per Backend File</h2>
  <table>
    <thead>
      <tr>
        <th>File</th>
        <th>Functions</th>
      </tr>
    </thead>
    <tbody>
$(
    (
        @($summary.file_function_counts) |
            ForEach-Object {
@"
<tr>
  <td><code>$(Escape-Html ([string]$_.file))</code></td>
  <td>$([int]$_.function_count)</td>
</tr>
"@
            }
    ) -join "`n"
)
    </tbody>
  </table>
</div>
<div class="panel">
  <h2>Top Cross-File Edges</h2>
  <table>
    <thead>
      <tr>
        <th>From</th>
        <th>To</th>
        <th>Calls</th>
      </tr>
    </thead>
    <tbody>
$(
    (
        @($summary.top_file_edges) |
            ForEach-Object {
@"
<tr>
  <td><code>$(Escape-Html ([string]$_.source_file))</code></td>
  <td><code>$(Escape-Html ([string]$_.target_file))</code></td>
  <td>$([int]$_.count)</td>
</tr>
"@
            }
    ) -join "`n"
)
    </tbody>
  </table>
</div>
<div class="panel">
  <h2>Top Fan-Out</h2>
  <table>
    <thead>
      <tr>
        <th>Function</th>
        <th>File</th>
        <th>Outgoing edges</th>
      </tr>
    </thead>
    <tbody>
$(($fanOutRows -join "`n"))
    </tbody>
  </table>
</div>
<div class="panel">
  <h2>Top Fan-In</h2>
  <table>
    <thead>
      <tr>
        <th>Function</th>
        <th>File</th>
        <th>Incoming edges</th>
      </tr>
    </thead>
    <tbody>
$(($fanInRows -join "`n"))
    </tbody>
  </table>
</div>
<div class="panel">
  <h2>Hidden External References</h2>
  <ul>
$(($externalItems -join "`n"))
  </ul>
</div>
<div class="panel">
  <h2>Notes</h2>
  <ul>
$(($noteItems -join "`n"))
  </ul>
</div>
<p class="footer"><a href="../index.html">Back to report index</a></p>
"@

        Write-ReportHtml -Path $reportPath -Title "Call Graph Report" -Body $body
        Write-ReportHtml -Path $outputPath -Title "Call Graph Report" -Body $body

        return [pscustomobject]@{
            Name = "Call Graph"
            Status = $(if ([string]::IsNullOrWhiteSpace($scipIndexerWarning)) { "ok" } else { "warn" })
            Notes = @($notes | Sort-Object -Unique)
            IndexPath = "callgraph/index.html"
            ErrorMessage = $null
        }
    }
    catch {
        $errorMessage = $_.Exception.Message
        $notes.Add("Call graph generation failed: $errorMessage")
        $body = @"
<h1>Call Graph Report Failed</h1>
<div class="panel">
  <p>The call graph step could not complete.</p>
  <p><code>$(Escape-Html $errorMessage)</code></p>
</div>
<p class="footer"><a href="../index.html">Back to report index</a></p>
"@
        Write-ReportHtml -Path $reportPath -Title "Call Graph Report Failed" -Body $body
        Write-ReportHtml -Path $outputPath -Title "Call Graph Report Failed" -Body $body

        return [pscustomobject]@{
            Name = "Call Graph"
            Status = "failed"
            Notes = @($notes)
            IndexPath = "callgraph/index.html"
            ErrorMessage = $errorMessage
        }
    }
}

function Invoke-FuzzCoverageReport {
    param(
        [hashtable]$SourceInventory
    )

    $notes = [System.Collections.Generic.List[string]]::new()
    $summaryPath = Join-Path $fuzzRoot "summary.json"
    $reportPath = Join-Path $fuzzRoot "index.html"
    $outputPath = Join-Path $fuzzRoot "output.html"
    $detailRoot = $fuzzRoot
    $artifactRoot = Join-Path $serverRoot "fuzz\artifacts"
    $generatedCorpusRoot = Join-Path $serverRoot "target\fuzz-generated-corpus"

    if (-not (Test-ToolAvailable -CommandName "cargo-llvm-cov")) {
        $notes.Add("Fuzz corpus coverage was skipped because cargo-llvm-cov is not installed.")
        $body = @"
<h1>Fuzz Corpus Coverage Unavailable</h1>
<div class="panel">
  <p>cargo-llvm-cov is not installed, so no fuzz corpus coverage report could be generated.</p>
  <p class="muted">Install it with <code>./scripts/install-tools.ps1</code>.</p>
</div>
<p class="footer"><a href="../index.html">Back to report index</a></p>
"@
        Write-ReportHtml -Path $reportPath -Title "Fuzz Corpus Coverage Unavailable" -Body $body
        Write-ReportHtml -Path $outputPath -Title "Fuzz Corpus Coverage Unavailable" -Body $body
        return [pscustomobject]@{
            Name = "Fuzz Coverage"
            Status = "failed"
            Notes = @($notes)
            IndexPath = "fuzz/index.html"
            ErrorMessage = "cargo-llvm-cov is not installed."
        }
    }

    try {
        if (Test-Path $fuzzRoot) {
            Remove-Item -Recurse -Force -Path $fuzzRoot
        }

        New-Item -ItemType Directory -Force -Path $fuzzRoot | Out-Null

        Invoke-CheckedCommand -Description "seed fuzz corpus" -Command {
            rustup run stable cargo run -p fuzz_seed_builder --quiet | Out-Host
        }
        Invoke-CheckedCommand -Description "cargo llvm-cov clean fuzz replay" -Command {
            rustup run stable cargo llvm-cov clean --workspace | Out-Host
        }
        foreach ($replaySuite in @(Get-FuzzReplaySuites)) {
            $description = "cargo llvm-cov fuzz replay ($($replaySuite.Package)/$($replaySuite.Test))"
            Invoke-CheckedCommand -Description $description -Command {
                rustup run stable cargo llvm-cov test -p $replaySuite.Package --test $replaySuite.Test --no-report | Out-Host
            }
        }
        Invoke-CheckedCommand -Description "cargo llvm-cov fuzz json report" -Command {
            rustup run stable cargo llvm-cov report --json --summary-only --output-path $summaryPath | Out-Host
        }
        Invoke-CheckedCommand -Description "cargo llvm-cov fuzz html report" -Command {
            rustup run stable cargo llvm-cov report --html --output-dir $detailRoot | Out-Host
        }

        $coverageJson = Get-Content -Path $summaryPath -Raw | ConvertFrom-Json
        $coverageData = $coverageJson.data | Select-Object -First 1
        $files = @()
        $coveredPaths = @{}

        foreach ($file in @($coverageData.files)) {
            $displayPath = Convert-ToDisplayPath -Path $file.filename
            $coveredPaths[$displayPath] = $true
            $files += [pscustomobject]@{
                DisplayPath = $displayPath
                NormalizedPath = $displayPath -replace '\\', '/'
                LinePercent = [double]$file.summary.lines.percent
                FunctionPercent = [double]$file.summary.functions.percent
                RegionPercent = [double]$file.summary.regions.percent
                CoveredLines = [int]$file.summary.lines.covered
                TotalLines = [int]$file.summary.lines.count
            }
        }

        $files = @($files | Sort-Object LinePercent, DisplayPath)
        $targetScopes = Get-FuzzTargetCatalog
        $primaryTargetScopes = @($targetScopes | Where-Object { $_.Primary })
        $primaryPathSet = @{}
        foreach ($targetScope in $primaryTargetScopes) {
            foreach ($path in @($targetScope.Paths)) {
                $primaryPathSet[$path] = $true
            }
        }
        $primaryRuntimeFiles = @($primaryPathSet.Keys | Sort-Object)
        $primaryRuntimeMetrics = @(
            foreach ($path in $primaryRuntimeFiles) {
                $fileMetric = $files | Where-Object { $_.NormalizedPath -eq $path } | Select-Object -First 1
                [pscustomobject]@{
                    DisplayPath = $path
                    IsHit = $null -ne $fileMetric
                    LinePercent = if ($null -ne $fileMetric) { [double]$fileMetric.LinePercent } else { 0.0 }
                    FunctionPercent = if ($null -ne $fileMetric) { [double]$fileMetric.FunctionPercent } else { 0.0 }
                    RegionPercent = if ($null -ne $fileMetric) { [double]$fileMetric.RegionPercent } else { 0.0 }
                    CoveredLines = if ($null -ne $fileMetric) { [int]$fileMetric.CoveredLines } else { 0 }
                    TotalLines = if ($null -ne $fileMetric) { [int]$fileMetric.TotalLines } else { 0 }
                }
            }
        )
        $supplementalReplayMetrics = @(
            $files |
                Where-Object { -not $primaryPathSet.ContainsKey($_.NormalizedPath) } |
                Sort-Object LinePercent, DisplayPath
        )

        $averagePrimaryLinePercent = if ($primaryRuntimeMetrics.Count -gt 0) {
            [double](($primaryRuntimeMetrics | Measure-Object -Property LinePercent -Average).Average)
        }
        else {
            0.0
        }
        $averagePrimaryFunctionPercent = if ($primaryRuntimeMetrics.Count -gt 0) {
            [double](($primaryRuntimeMetrics | Measure-Object -Property FunctionPercent -Average).Average)
        }
        else {
            0.0
        }
        $averagePrimaryRegionPercent = if ($primaryRuntimeMetrics.Count -gt 0) {
            [double](($primaryRuntimeMetrics | Measure-Object -Property RegionPercent -Average).Average)
        }
        else {
            0.0
        }

        $scopeRows = foreach ($scope in $targetScopes) {
            $corpusStats = Get-FuzzCorpusStats -ServerRoot $serverRoot -Target $scope.Target

@"
<tr>
  <td><code>$(Escape-Html $scope.Target)</code></td>
  <td>$(if ($scope.Primary) { "Primary" } else { "Supplemental" })</td>
  <td>$($corpusStats.SeedFiles.Count)</td>
  <td>$($corpusStats.DiscoveredFiles.Count)</td>
  <td><code>$(Escape-Html (Convert-ToDisplayPath -Path $corpusStats.SeedDir))</code></td>
  <td><code>$(Escape-Html (Convert-ToDisplayPath -Path $corpusStats.GeneratedDir))</code></td>
  <td>$(Escape-Html $scope.Scope)</td>
  <td>$(Escape-Html $scope.Description)</td>
</tr>
"@
        }

        $fileRows = foreach ($file in $primaryRuntimeMetrics) {
@"
<tr>
  <td><code>$(Escape-Html $file.DisplayPath)</code></td>
  <td>$(if ($file.IsHit) { 'Yes' } else { 'No' })</td>
  <td>$(Format-Percent -Value $file.LinePercent)</td>
  <td>$($file.CoveredLines) / $($file.TotalLines)</td>
  <td>$(Format-Percent -Value $file.FunctionPercent)</td>
  <td>$(Format-Percent -Value $file.RegionPercent)</td>
</tr>
"@
        }

        $supplementalRows = foreach ($file in $supplementalReplayMetrics) {
@"
<tr>
  <td><code>$(Escape-Html $file.DisplayPath)</code></td>
  <td>$(Format-Percent -Value $file.LinePercent)</td>
  <td>$($file.CoveredLines) / $($file.TotalLines)</td>
  <td>$(Format-Percent -Value $file.FunctionPercent)</td>
  <td>$(Format-Percent -Value $file.RegionPercent)</td>
</tr>
"@
        }

        $coveredPrimaryCount = @($primaryRuntimeMetrics | Where-Object { $_.IsHit }).Count
        $uncoveredRows = foreach ($file in ($primaryRuntimeMetrics | Where-Object { -not $_.IsHit })) {
@"
<tr>
  <td><code>$(Escape-Html $file.DisplayPath)</code></td>
  <td>Not hit by the current replay corpus yet.</td>
</tr>
"@
        }

        $primaryFileHitPercent = if ($primaryRuntimeMetrics.Count -eq 0) {
            0.0
        }
        else {
            ($coveredPrimaryCount / $primaryRuntimeMetrics.Count) * 100.0
        }
        $seededTargetCount = @($primaryTargetScopes | Where-Object {
            (Get-FuzzCorpusStats -ServerRoot $serverRoot -Target $_.Target).SeedFiles.Count -gt 0
        }).Count
        $seededTargetPercent = if ($primaryTargetScopes.Count -eq 0) {
            0.0
        }
        else {
            ($seededTargetCount / $primaryTargetScopes.Count) * 100.0
        }
        $discoveredTargetCount = @($primaryTargetScopes | Where-Object {
            (Get-FuzzCorpusStats -ServerRoot $serverRoot -Target $_.Target).DiscoveredFiles.Count -gt 0
        }).Count
        $discoveredTargetPercent = if ($primaryTargetScopes.Count -eq 0) {
            0.0
        }
        else {
            ($discoveredTargetCount / $primaryTargetScopes.Count) * 100.0
        }
        $findings = @(Get-FuzzArtifactFindings -ArtifactRoot $artifactRoot -TargetCatalog $targetScopes)
        $cleanFindingsPercent = if ($findings.Count -eq 0) {
            100.0
        }
        else {
            0.0
        }
        $scoreSummary = New-ScoreSummary `
            -Score (($averagePrimaryLinePercent * 0.25) + ($averagePrimaryFunctionPercent * 0.2) + ($averagePrimaryRegionPercent * 0.1) + ($primaryFileHitPercent * 0.15) + ($seededTargetPercent * 0.1) + ($discoveredTargetPercent * 0.1) + ($cleanFindingsPercent * 0.1)) `
            -Formula "25% primary runtime line + 20% primary runtime function + 10% primary runtime region replay coverage + 15% primary runtime file hit rate + 10% seeded target coverage + 10% discovered-corpus target coverage + 10% no saved findings" `
            -Breakdown @(
                "Primary runtime lines: $(Format-Percent -Value $averagePrimaryLinePercent)",
                "Primary runtime functions: $(Format-Percent -Value $averagePrimaryFunctionPercent)",
                "Primary runtime regions: $(Format-Percent -Value $averagePrimaryRegionPercent)",
                "Primary runtime files hit: $(Format-Percent -Value $primaryFileHitPercent)",
                "Seeded primary targets: $(Format-Percent -Value $seededTargetPercent)",
                "Discovered-corpus primary targets: $(Format-Percent -Value $discoveredTargetPercent)",
                "No saved findings: $(Format-Percent -Value $cleanFindingsPercent)"
            )

        $notes.Add("This report measures replay coverage over the checked-in seed corpus plus any discovered corpus present under server/target/fuzz-generated-corpus.")
        $notes.Add("Saved crash artifacts are reviewed on this page under Findings and reproductions, and are stored under server/fuzz/artifacts.")
        $notes.Add("Seed corpora are generated by fuzz_seed_builder and replayed through the same decode and ingress APIs that the fuzz targets call.")
        $notes.Add("Headline fuzz scoring is scoped to the primary network-ingress files mapped from the ingress fuzz targets. Additional replay-hit files remain visible below as supplemental coverage.")
        $notes.Add("Structured round-trip fuzz targets exist for selected network packet types, but the headline score remains focused on ingress decode and validation.")
        $notes.Add("This Windows/MSVC host does not currently emit native cargo fuzz coverage HTML for this repo, so corpus replay coverage is used instead.")
        if ($discoveredTargetCount -gt 0) {
            $notes.Add("$discoveredTargetCount primary fuzz target(s) currently have a discovered corpus under server/target/fuzz-generated-corpus.")
        }
        else {
            $notes.Add("No discovered corpus is currently present under server/target/fuzz-generated-corpus. Local Windows runs will usually show seed-only replay until Linux CI or Docker runs a live cargo fuzz campaign.")
            $notes.Add("Because discovered-corpus target coverage contributes 10% of the fuzzing score, any run with zero discovered corpus is capped at 90/100 even if every replay metric is perfect.")
        }
        if ($findings.Count -eq 0) {
            $notes.Add("No saved crash artifacts are present under server/fuzz/artifacts.")
        }
        else {
            $notes.Add("$($findings.Count) saved fuzz finding artifact(s) were found under server/fuzz/artifacts.")
        }

        $findingRows = foreach ($finding in $findings) {
@"
<tr>
  <td><code>$(Escape-Html $finding.Target)</code></td>
  <td><code>$(Escape-Html $finding.FileName)</code></td>
  <td><code>$(Escape-Html $finding.RelativePath)</code></td>
  <td>$([int]$finding.Size)</td>
  <td>$(Escape-Html ($finding.LastWriteTime.ToString("u")))</td>
  <td><code>$(Escape-Html $finding.Scope)</code></td>
  <td><code>$(Escape-Html $finding.Sha256)</code></td>
  <td><code>$(Escape-Html $finding.HexPreview)</code></td>
  <td><code>$(Escape-Html $finding.ReproCommand)</code></td>
</tr>
"@
        }

        $body = @"
<h1>Fuzz Coverage</h1>
<p class="muted">This report replays the checked-in seed corpus plus any discovered corpus under <code>server/target/fuzz-generated-corpus</code> through the backend decode and ingress surfaces, then measures the exercised lines with <code>cargo llvm-cov</code>. Detailed line-by-line output: <a href="./html/index.html">fuzz/html/index.html</a>.</p>
<div class="grid">
  <div class="metric"><span class="muted">Fuzzing score</span><strong>$(Format-Score -Score $scoreSummary.Score) $(Format-GradeBadge -Grade $scoreSummary.Grade)</strong><div class="detail">$(Escape-Html $scoreSummary.Formula)</div></div>
  <div class="metric"><span class="muted">Primary line coverage</span><strong>$(Format-Percent -Value $averagePrimaryLinePercent)</strong></div>
  <div class="metric"><span class="muted">Primary function coverage</span><strong>$(Format-Percent -Value $averagePrimaryFunctionPercent)</strong></div>
  <div class="metric"><span class="muted">Primary region coverage</span><strong>$(Format-Percent -Value $averagePrimaryRegionPercent)</strong></div>
  <div class="metric"><span class="muted">Primary files hit</span><strong>$(Format-Percent -Value $primaryFileHitPercent)</strong></div>
  <div class="metric"><span class="muted">Coverage mode</span><strong>Seed + discovered replay</strong></div>
  <div class="metric"><span class="muted">Seeded primary targets</span><strong>$(Format-Percent -Value $seededTargetPercent)</strong></div>
  <div class="metric"><span class="muted">Discovered-corpus targets</span><strong>$(Format-Percent -Value $discoveredTargetPercent)</strong></div>
  <div class="metric"><span class="muted">Saved findings</span><strong>$($findings.Count)</strong></div>
  <div class="metric"><span class="muted">Findings directory</span><strong><code>server/fuzz/artifacts</code></strong></div>
</div>
<div class="panel">
  <h2>Fuzz targets and scope</h2>
  <table>
    <thead>
      <tr>
        <th>Target</th>
        <th>Scope</th>
        <th>Seed files</th>
        <th>Discovered files</th>
        <th>Seed corpus directory</th>
        <th>Discovered corpus directory</th>
        <th>Expected source scope</th>
        <th>Focus</th>
      </tr>
    </thead>
    <tbody>
$(($scopeRows -join "`n"))
    </tbody>
  </table>
</div>
<div class="panel">
  <h2>Primary runtime file replay coverage</h2>
  <table>
    <thead>
      <tr>
        <th>File</th>
        <th>Hit</th>
        <th>Lines</th>
        <th>Covered lines</th>
        <th>Functions</th>
        <th>Regions</th>
      </tr>
    </thead>
    <tbody>
$(($fileRows -join "`n"))
    </tbody>
  </table>
</div>
<div class="panel">
  <h2>Primary runtime files not currently hit</h2>
  <table>
    <thead>
      <tr>
        <th>File</th>
        <th>Reason</th>
      </tr>
    </thead>
    <tbody>
$(if ($uncoveredRows) { $uncoveredRows -join "`n" } else { '<tr><td colspan="2">All primary ingress-runtime files were hit by the current replay corpus.</td></tr>' })
    </tbody>
  </table>
</div>
$(if ($supplementalRows) {
@"
<div class="panel">
  <h2>Supplemental replay-hit files</h2>
  <p class="muted">These files were touched while replaying the corpus, but they are not the primary runtime files scored by the fuzz report.</p>
  <table>
    <thead>
      <tr>
        <th>File</th>
        <th>Lines</th>
        <th>Covered lines</th>
        <th>Functions</th>
        <th>Regions</th>
      </tr>
    </thead>
    <tbody>
$(($supplementalRows -join "`n"))
    </tbody>
  </table>
</div>
"@
} else { "" })
<div class="panel">
  <h2>Findings and reproductions</h2>
  <p class="muted">Saved crash artifacts live under <code>server/fuzz/artifacts/&lt;target&gt;/</code>. Review them here and rerun the listed reproduction command to confirm and debug each finding.</p>
$(if ($findingRows) {
@"
  <table>
    <thead>
      <tr>
        <th>Target</th>
        <th>Artifact</th>
        <th>Path</th>
        <th>Bytes</th>
        <th>Updated</th>
        <th>Expected scope</th>
        <th>SHA-256</th>
        <th>Preview</th>
        <th>Reproduce</th>
      </tr>
    </thead>
    <tbody>
$(($findingRows -join "`n"))
    </tbody>
  </table>
"@
} else {
@"
  <p>No saved fuzz findings are currently present under <code>server/fuzz/artifacts</code>.</p>
  <p class="muted">Current coverage is based on replaying checked-in seeds plus any discovered corpus already present. To create finding artifacts or grow the discovered corpus, run a bounded live <code>cargo fuzz run &lt;target&gt;</code> campaign in Linux CI, Docker, or WSL.</p>
"@
})
</div>
<p class="footer"><a href="../index.html">Back to report index</a></p>
"@

        Write-ReportHtml -Path $reportPath -Title "Fuzz Coverage" -Body $body
        Write-ReportHtml -Path $outputPath -Title "Fuzz Coverage" -Body $body

        return [pscustomobject]@{
            Name = "Fuzz Coverage"
            Status = "ok"
            Notes = @($notes | Sort-Object -Unique)
            IndexPath = "fuzz/index.html"
            ErrorMessage = $null
            ScoreSummary = $scoreSummary
            Summary = [pscustomobject]@{
                Lines = $averagePrimaryLinePercent
                Functions = $averagePrimaryFunctionPercent
                Regions = $averagePrimaryRegionPercent
                CoreFileHitPercent = $primaryFileHitPercent
                PrimaryFileHitPercent = $primaryFileHitPercent
                SeededTargetPercent = $seededTargetPercent
                DiscoveredTargetPercent = $discoveredTargetPercent
                CleanFindingsPercent = $cleanFindingsPercent
                Findings = $findings.Count
            }
        }
    }
    catch {
        $errorMessage = $_.Exception.Message
        $notes.Add("Fuzz corpus coverage generation failed: $errorMessage")
        $body = @"
<h1>Fuzz Coverage Failed</h1>
<div class="panel">
  <p>The fuzz corpus coverage step could not complete.</p>
  <p><code>$(Escape-Html $errorMessage)</code></p>
</div>
<p class="footer"><a href="../index.html">Back to report index</a></p>
"@
        Write-ReportHtml -Path $reportPath -Title "Fuzz Coverage Failed" -Body $body
        Write-ReportHtml -Path $outputPath -Title "Fuzz Coverage Failed" -Body $body

        return [pscustomobject]@{
            Name = "Fuzz Coverage"
            Status = "failed"
            Notes = @($notes)
            IndexPath = "fuzz/index.html"
            ErrorMessage = $errorMessage
        }
    }
}

function Invoke-HardeningQueueReport {
    param(
        [hashtable]$SourceInventory
    )

    $notes = [System.Collections.Generic.List[string]]::new()
    $reportPath = Join-Path $hardeningRoot "index.html"
    $outputPath = Join-Path $hardeningRoot "output.html"
    $markdownPath = Join-Path $hardeningRoot "llm_todo.md"
    $jsonPath = Join-Path $hardeningRoot "llm_todo.json"
    $artifactRoot = Join-Path $serverRoot "fuzz\artifacts"

    try {
        if (Test-Path $hardeningRoot) {
            Remove-Item -Recurse -Force -Path $hardeningRoot
        }
        New-Item -ItemType Directory -Force -Path $hardeningRoot | Out-Null

        $fuzzSummaryPath = Join-Path $fuzzRoot "summary.json"
        if (-not (Test-Path $fuzzSummaryPath)) {
            throw "Fuzz summary is missing at $fuzzSummaryPath. Generate the fuzz report first."
        }

        $complexityDataRoot = Join-Path $complexityRoot "data"
        if (-not (Test-Path $complexityDataRoot)) {
            throw "Complexity analyzer output is missing under $complexityDataRoot. Generate the complexity report first."
        }

        $backendCoreFiles = @(Get-BackendCoreRuntimeFiles -SourceInventory $SourceInventory)
        $backendCoreFileSet = @{}
        foreach ($path in $backendCoreFiles) {
            $backendCoreFileSet[$path] = $true
        }
        $targetCatalog = Get-FuzzTargetCatalog

        $fuzzJson = Get-Content -Path $fuzzSummaryPath -Raw | ConvertFrom-Json
        $fuzzCoverageData = $fuzzJson.data | Select-Object -First 1
        $fuzzByPath = @{}
        foreach ($file in @($fuzzCoverageData.files)) {
            $displayPath = Convert-ToDisplayPath -Path $file.filename
            $normalizedPath = $displayPath -replace '\\', '/'
            $fuzzByPath[$normalizedPath] = [pscustomobject]@{
                DisplayPath = $displayPath
                LinePercent = [double]$file.summary.lines.percent
                FunctionPercent = [double]$file.summary.functions.percent
                RegionPercent = [double]$file.summary.regions.percent
                CoveredLines = [int]$file.summary.lines.covered
                TotalLines = [int]$file.summary.lines.count
            }
        }

        $fileMetrics = @()
        $functionMetrics = @()
        foreach ($jsonFile in Get-ChildItem -Path $complexityDataRoot -Recurse -File -Filter *.json) {
            $jsonText = Get-Content -Path $jsonFile.FullName -Raw
            $jsonText = $jsonText.Replace('"N1":', '"N1_upper":').Replace('"N2":', '"N2_upper":')
            $metrics = $jsonText | ConvertFrom-Json
            $displayPath = Convert-ToDisplayPath -Path $metrics.name
            $normalizedPath = $displayPath -replace '\\', '/'
            if (-not $backendCoreFileSet.ContainsKey($normalizedPath)) {
                continue
            }

            $fileMetrics += [pscustomobject]@{
                DisplayPath = $displayPath
                NormalizedPath = $normalizedPath
                Mi = [double]$metrics.metrics.mi.mi_visual_studio
                MiGrade = Get-MaintainabilityGrade -Score ([double]$metrics.metrics.mi.mi_visual_studio)
                Cyclomatic = [double]$metrics.metrics.cyclomatic.sum
                Cognitive = [double]$metrics.metrics.cognitive.sum
                FunctionCount = [int]$metrics.metrics.nom.functions
                Sloc = [double]$metrics.metrics.loc.sloc
            }

            foreach ($node in @($metrics.spaces)) {
                $functionMetrics += Get-ComplexityFunctions -Node $node -FilePath $displayPath
            }
        }

        foreach ($function in $functionMetrics) {
            Add-Member -InputObject $function -NotePropertyName NormalizedPath -NotePropertyValue (($function.FilePath -replace '\\', '/')) -Force
            Add-Member -InputObject $function -NotePropertyName CyclomaticGrade -NotePropertyValue (Get-CyclomaticGrade -Score $function.Cyclomatic) -Force
            Add-Member -InputObject $function -NotePropertyName MiGrade -NotePropertyValue (Get-MaintainabilityGrade -Score $function.Mi) -Force
        }

        $functionMetrics = @(
            $functionMetrics |
                Where-Object {
                    $backendCoreFileSet.ContainsKey($_.NormalizedPath) -and
                    $_.Name -notmatch '(^test_|::tests::|::prop_)'
                } |
                Sort-Object @{ Expression = "Cyclomatic"; Descending = $true }, @{ Expression = "Cognitive"; Descending = $true }, FilePath, Name
        )

        $fileFunctionSummaries = @{}
        foreach ($group in ($functionMetrics | Group-Object NormalizedPath)) {
            $worstFunction = $group.Group | Sort-Object @{ Expression = "Cyclomatic"; Descending = $true }, @{ Expression = "Cognitive"; Descending = $true }, Name | Select-Object -First 1
            $averageCyclomatic = [double](($group.Group | Measure-Object -Property Cyclomatic -Average).Average)
            $fileFunctionSummaries[$group.Name] = [pscustomobject]@{
                WorstCyclomatic = [double]$worstFunction.Cyclomatic
                WorstGrade = [string]$worstFunction.CyclomaticGrade
                AverageCyclomatic = $averageCyclomatic
                AverageGrade = Get-CyclomaticGrade -Score $averageCyclomatic
                HotFunctions = @($group.Group | Sort-Object @{ Expression = "Cyclomatic"; Descending = $true }, @{ Expression = "Cognitive"; Descending = $true }, Name | Select-Object -First 3)
            }
        }

        $fileTasks = foreach ($backendPath in $backendCoreFiles) {
            $fileMetric = $fileMetrics | Where-Object { $_.NormalizedPath -eq $backendPath } | Select-Object -First 1
            $fuzzMetric = if ($fuzzByPath.ContainsKey($backendPath)) { $fuzzByPath[$backendPath] } else { $null }
            $functionSummary = if ($fileFunctionSummaries.ContainsKey($backendPath)) { $fileFunctionSummaries[$backendPath] } else { $null }

            if (($null -eq $fileMetric) -and ($null -eq $fuzzMetric)) {
                continue
            }

            $displayPath = if ($null -ne $fileMetric) {
                $fileMetric.DisplayPath
            }
            elseif ($null -ne $fuzzMetric) {
                $fuzzMetric.DisplayPath
            }
            else {
                $backendPath -replace '/', '\'
            }

            $worstFunctionRisk = if (($null -ne $functionSummary) -and -not [string]::IsNullOrWhiteSpace($functionSummary.WorstGrade)) {
                100.0 - (Get-CyclomaticGradeScore -Grade $functionSummary.WorstGrade)
            }
            else {
                0.0
            }
            $averageFunctionRisk = if (($null -ne $functionSummary) -and -not [string]::IsNullOrWhiteSpace($functionSummary.AverageGrade)) {
                100.0 - (Get-CyclomaticGradeScore -Grade $functionSummary.AverageGrade)
            }
            else {
                0.0
            }
            $miRisk = if ($null -ne $fileMetric) {
                100.0 - (Clamp-Score -Value $fileMetric.Mi)
            }
            else {
                0.0
            }
            $fuzzLinePercent = if ($null -ne $fuzzMetric) { [double]$fuzzMetric.LinePercent } else { 0.0 }
            $fuzzFunctionPercent = if ($null -ne $fuzzMetric) { [double]$fuzzMetric.FunctionPercent } else { 0.0 }
            $priorityScore = Clamp-Score -Value (
                ($worstFunctionRisk * 0.35) +
                ($averageFunctionRisk * 0.20) +
                ($miRisk * 0.15) +
                ((100.0 - $fuzzLinePercent) * 0.20) +
                ((100.0 - $fuzzFunctionPercent) * 0.10)
            )

            $reasons = [System.Collections.Generic.List[string]]::new()
            if ($null -ne $functionSummary) {
                $reasons.Add("Worst function CC $("{0:N0}" -f $functionSummary.WorstCyclomatic) ($($functionSummary.WorstGrade))")
                $reasons.Add("Average function CC $("{0:N2}" -f $functionSummary.AverageCyclomatic) ($($functionSummary.AverageGrade))")
            }
            if ($null -ne $fileMetric) {
                $reasons.Add("Maintainability index $("{0:N2}" -f $fileMetric.Mi) ($($fileMetric.MiGrade))")
            }
            if ($null -ne $fuzzMetric) {
                $reasons.Add("Fuzz line coverage $(Format-Percent -Value $fuzzLinePercent)")
                $reasons.Add("Fuzz function coverage $(Format-Percent -Value $fuzzFunctionPercent)")
            }
            else {
                $reasons.Add("No direct fuzz coverage entry yet")
            }

            $actions = [System.Collections.Generic.List[string]]::new()
            if (($null -ne $functionSummary) -and ($functionSummary.WorstCyclomatic -ge 20.0)) {
                $actions.Add("Split the worst branching paths into smaller internal helpers before adding new behavior.")
            }
            if (($null -eq $fuzzMetric) -or ($fuzzLinePercent -lt 50.0) -or ($fuzzFunctionPercent -lt 50.0)) {
                $actions.Add("Increase fuzz coverage or corpus replay coverage for this file until malformed-input handling has stronger branch coverage.")
            }
            $actions.Add("Add or extend focused positive and negative tests for every touched branch.")

            $hotFunctions = if ($null -ne $functionSummary) {
                Get-OptionalArrayPropertyValue -InputObject $functionSummary -PropertyName "HotFunctions"
            }
            else {
                @()
            }

            [pscustomobject]@{
                Kind = "file_cleanup"
                Title = $displayPath
                DisplayPath = $displayPath
                NormalizedPath = $backendPath
                PriorityScore = $priorityScore
                Mi = if ($null -ne $fileMetric) { [double]$fileMetric.Mi } else { $null }
                MiGrade = if ($null -ne $fileMetric) { [string]$fileMetric.MiGrade } else { $null }
                WorstFunctionCyclomatic = if ($null -ne $functionSummary) { [double]$functionSummary.WorstCyclomatic } else { $null }
                WorstFunctionGrade = if ($null -ne $functionSummary) { [string]$functionSummary.WorstGrade } else { $null }
                AverageFunctionCyclomatic = if ($null -ne $functionSummary) { [double]$functionSummary.AverageCyclomatic } else { $null }
                AverageFunctionGrade = if ($null -ne $functionSummary) { [string]$functionSummary.AverageGrade } else { $null }
                FuzzLinePercent = $fuzzLinePercent
                FuzzFunctionPercent = $fuzzFunctionPercent
                FuzzRegionPercent = if ($null -ne $fuzzMetric) { [double]$fuzzMetric.RegionPercent } else { 0.0 }
                HotFunctions = @($hotFunctions)
                Reasons = @($reasons)
                Actions = @($actions)
                Prompt = Get-BackendCorePrompt -FilePath $displayPath -HotFunctions @($hotFunctions) -FuzzLinePercent $fuzzLinePercent -FuzzFunctionPercent $fuzzFunctionPercent
            }
        }

        $fileTasks = @(
            $fileTasks |
                Sort-Object @{ Expression = "PriorityScore"; Descending = $true }, @{ Expression = "WorstFunctionCyclomatic"; Descending = $true }, DisplayPath
        )

        $findings = @(Get-FuzzArtifactFindings -ArtifactRoot $artifactRoot -TargetCatalog $targetCatalog)
        $findingTasks = @(
            foreach ($finding in ($findings | Sort-Object @{ Expression = "LastWriteTime"; Descending = $true }, FileName)) {
            [pscustomobject]@{
                Kind = "crash_finding"
                Title = "$($finding.Target): $($finding.FileName)"
                DisplayPath = $finding.RelativePath
                NormalizedPath = $null
                PriorityScore = 100.0
                Mi = $null
                MiGrade = $null
                WorstFunctionCyclomatic = $null
                WorstFunctionGrade = $null
                AverageFunctionCyclomatic = $null
                AverageFunctionGrade = $null
                FuzzLinePercent = $null
                FuzzFunctionPercent = $null
                FuzzRegionPercent = $null
                HotFunctions = @()
                Reasons = @(
                    "Saved fuzz crash artifact exists",
                    "Target: $($finding.Target)",
                    "Expected scope: $($finding.Scope)",
                    "Updated: $($finding.LastWriteTime.ToString('u'))",
                    "Bytes: $($finding.Size)",
                    "SHA-256: $($finding.Sha256)"
                )
                Actions = @(
                    "Reproduce the crash with the saved artifact before refactoring anything around it.",
                    "Minimize and understand the malformed input, then add a regression test and keep the artifact as a seed.",
                    "Fix the parser, validator, or state transition defensively without trusting any network field."
                )
                Prompt = "Investigate the saved fuzz crash artifact $($finding.RelativePath) for target $($finding.Target). Expected scope: $($finding.Scope). Reproduce it with: $($finding.ReproCommand). Preserve protocol behavior where valid, reject malformed input defensively, add a regression test, and keep or minimize the artifact as a fuzz seed. SHA-256: $($finding.Sha256). Hex preview: $($finding.HexPreview)."
                Finding = $finding
            }
            }
        )

        $queueItems = @(@($findingTasks) + @($fileTasks))
        for ($index = 0; $index -lt $queueItems.Count; $index += 1) {
            Add-Member -InputObject $queueItems[$index] -NotePropertyName Rank -NotePropertyValue ($index + 1) -Force
        }

        $notes.Add("This queue is generated from the current complexity and fuzz reports. It is intended as a repair plan, not an automatic code change.")
        $notes.Add("Backend core scope is limited to game_api, game_domain, game_lobby, game_match, game_net, and game_sim runtime files.")
        $notes.Add("Priority score weights: 35% worst-function risk, 20% average-function risk, 15% maintainability risk, 20% missing fuzz line coverage, 10% missing fuzz function coverage.")
        $notes.Add("Saved fuzz findings, when present, are placed at the top of the queue ahead of general refactoring.")

        $queueRows = foreach ($task in $queueItems) {
            $kindLabel = if ($task.Kind -eq "crash_finding") { "Crash finding" } else { "Code cleanup" }
            $taskHotFunctions = Get-OptionalArrayPropertyValue -InputObject $task -PropertyName "HotFunctions"
            $hotspotLabel = if ($task.Kind -eq "crash_finding") {
                "Target: $(Escape-Html $task.Finding.Target)<br />Scope: $(Escape-Html $task.Finding.Scope)<br />SHA-256: <code>$(Escape-Html $task.Finding.Sha256)</code><br />Preview: <code>$(Escape-Html $task.Finding.HexPreview)</code>"
            }
            elseif ($taskHotFunctions.Count -gt 0) {
                ($taskHotFunctions | ForEach-Object { "$($_.Name) (CC $([int]$_.Cyclomatic))" }) -join "<br />"
            }
            else {
                '<span class="muted">No function hotspot extracted.</span>'
            }

@"
<tr>
  <td>$($task.Rank)</td>
  <td>$(Escape-Html $kindLabel)</td>
  <td><code>$(Escape-Html $task.DisplayPath)</code></td>
  <td>$("{0:N2}" -f $task.PriorityScore)</td>
  <td>$(Escape-Html ($task.Reasons -join " | "))</td>
  <td>$hotspotLabel</td>
  <td>$(Escape-Html ($task.Actions -join " "))</td>
</tr>
"@
        }

        $findingRows = foreach ($finding in $findings) {
@"
<tr>
  <td><code>$(Escape-Html $finding.Target)</code></td>
  <td><code>$(Escape-Html $finding.FileName)</code></td>
  <td><code>$(Escape-Html $finding.RelativePath)</code></td>
  <td>$(Escape-Html ($finding.LastWriteTime.ToString("u")))</td>
  <td>$([int]$finding.Size)</td>
  <td><code>$(Escape-Html $finding.Scope)</code></td>
  <td><code>$(Escape-Html $finding.Sha256)</code></td>
  <td><code>$(Escape-Html $finding.HexPreview)</code></td>
  <td><code>$(Escape-Html $finding.AsciiPreview)</code></td>
  <td><code>$(Escape-Html $finding.ReproCommand)</code></td>
</tr>
"@
        }

        $markdownLines = [System.Collections.Generic.List[string]]::new()
        $markdownLines.Add("# Backend Hardening Queue")
        $markdownLines.Add("")
        $markdownLines.Add('Generated from `server/target/reports/complexity` and `server/target/reports/fuzz`.')
        $markdownLines.Add("")
        $markdownLines.Add("Use this as a prioritized repair queue for another LLM.")
        $markdownLines.Add("")
        $markdownLines.Add("Rules:")
        $markdownLines.Add("- Preserve behavior and packet formats unless tests or docs require a deliberate change.")
        $markdownLines.Add("- For every touched function, add or extend focused positive and negative tests.")
        $markdownLines.Add("- For network-facing code, also extend fuzz seeds or corpus replay coverage.")
        $markdownLines.Add("")

        if ($findings.Count -gt 0) {
            $markdownLines.Add("## Immediate fuzz findings")
            $markdownLines.Add("")
            foreach ($finding in $findings) {
                $markdownLines.Add('- `' + $finding.Target + '`: `' + $finding.RelativePath + '`')
                $markdownLines.Add('  Updated: `' + $finding.LastWriteTime.ToString("u") + '`')
                $markdownLines.Add('  Bytes: `' + $finding.Size + '`')
                $markdownLines.Add('  Expected scope: `' + $finding.Scope + '`')
                $markdownLines.Add('  SHA-256: `' + $finding.Sha256 + '`')
                $markdownLines.Add('  Hex preview: `' + $finding.HexPreview + '`')
                $markdownLines.Add('  Reproduce with: `' + $finding.ReproCommand + '`')
            }
            $markdownLines.Add("")
        }

        $markdownLines.Add("## Prioritized file queue")
        $markdownLines.Add("")
        foreach ($task in $queueItems) {
            $taskHotFunctions = Get-OptionalArrayPropertyValue -InputObject $task -PropertyName "HotFunctions"
            $markdownLines.Add("### $($task.Rank). $($task.DisplayPath)")
            $markdownLines.Add("")
            $markdownLines.Add("- Kind: $($task.Kind)")
            $markdownLines.Add("- Priority score: $("{0:N2}" -f $task.PriorityScore)")
            foreach ($reason in $task.Reasons) {
                $markdownLines.Add("- Why: $reason")
            }
            if ($task.Kind -eq "crash_finding") {
                $markdownLines.Add('- Debug: target `' + $task.Finding.Target + '` | scope `' + $task.Finding.Scope + '` | sha256 `' + $task.Finding.Sha256 + '`')
                $markdownLines.Add('- Debug: hex preview `' + $task.Finding.HexPreview + '`')
                $markdownLines.Add('- Debug: reproduce with `' + $task.Finding.ReproCommand + '`')
            }
            elseif ($taskHotFunctions.Count -gt 0) {
                foreach ($function in $taskHotFunctions) {
                    $markdownLines.Add('- Hot function: `' + $function.Name + '` | CC ' + ([int]$function.Cyclomatic) + ' (' + $function.CyclomaticGrade + ') | MI ' + ('{0:N2}' -f $function.Mi) + ' (' + $function.MiGrade + ') | lines ' + $function.StartLine + '-' + $function.EndLine)
                }
            }
            foreach ($action in $task.Actions) {
                $markdownLines.Add("- Do: $action")
            }
            $markdownLines.Add("- Prompt: $($task.Prompt)")
            $markdownLines.Add("")
        }
        Set-Content -Path $markdownPath -Value ($markdownLines -join "`r`n") -Encoding UTF8

        $jsonPayload = [pscustomobject]@{
            generated_at = (Get-Date).ToUniversalTime().ToString("u")
            commit = Get-GitValue -CommandArgs @("rev-parse", "--short", "HEAD") -Fallback "unknown"
            based_on = [pscustomobject]@{
                complexity_report = "server/target/reports/complexity/output.html"
                fuzz_report = "server/target/reports/fuzz/output.html"
            }
            findings = @($findings)
            queue = @(
                $queueItems | ForEach-Object {
                    $taskHotFunctions = Get-OptionalArrayPropertyValue -InputObject $_ -PropertyName "HotFunctions"
                    [pscustomobject]@{
                        rank = $_.Rank
                        kind = $_.Kind
                        title = $_.Title
                        file = $_.DisplayPath
                        priority_score = [math]::Round([double]$_.PriorityScore, 2)
                        reasons = @($_.Reasons)
                        actions = @($_.Actions)
                        prompt = $_.Prompt
                        complexity = [pscustomobject]@{
                            maintainability_index = $_.Mi
                            maintainability_grade = $_.MiGrade
                            worst_function_cyclomatic = $_.WorstFunctionCyclomatic
                            worst_function_grade = $_.WorstFunctionGrade
                            average_function_cyclomatic = $_.AverageFunctionCyclomatic
                            average_function_grade = $_.AverageFunctionGrade
                        }
                        fuzz = [pscustomobject]@{
                            line_percent = $_.FuzzLinePercent
                            function_percent = $_.FuzzFunctionPercent
                            region_percent = $_.FuzzRegionPercent
                        }
                        hot_functions = @(
                            $taskHotFunctions | ForEach-Object {
                                [pscustomobject]@{
                                    name = $_.Name
                                    cyclomatic = $_.Cyclomatic
                                    cyclomatic_grade = $_.CyclomaticGrade
                                    cognitive = $_.Cognitive
                                    maintainability_index = $_.Mi
                                    maintainability_grade = $_.MiGrade
                                    start_line = $_.StartLine
                                    end_line = $_.EndLine
                                }
                            }
                        )
                        finding = if ($_.Kind -eq "crash_finding") {
                            [pscustomobject]@{
                                target = $_.Finding.Target
                                file_name = $_.Finding.FileName
                                relative_path = $_.Finding.RelativePath
                                updated_utc = $_.Finding.LastWriteTime.ToString("u")
                                size_bytes = $_.Finding.Size
                                scope = $_.Finding.Scope
                                description = $_.Finding.Description
                                sha256 = $_.Finding.Sha256
                                hex_preview = $_.Finding.HexPreview
                                ascii_preview = $_.Finding.AsciiPreview
                                reproduce = $_.Finding.ReproCommand
                            }
                        } else { $null }
                    }
                }
            )
        }
        $jsonPayload | ConvertTo-Json -Depth 8 | Set-Content -Path $jsonPath -Encoding UTF8

        $body = @"
<h1>Backend Hardening Queue</h1>
<p class="muted">Generated from the current complexity and fuzz reports. This is a prioritized cleanup queue for future work, not an automatic refactor.</p>
<div class="grid">
  <div class="metric"><span class="muted">Queued items</span><strong>$($queueItems.Count)</strong></div>
  <div class="metric"><span class="muted">Saved fuzz findings</span><strong>$($findings.Count)</strong></div>
  <div class="metric"><span class="muted">Top priority</span><strong>$(if ($queueItems.Count -gt 0) { Escape-Html $queueItems[0].DisplayPath } else { 'n/a' })</strong></div>
  <div class="metric"><span class="muted">Markdown handoff</span><strong><a href="./llm_todo.md">Open llm_todo.md</a></strong></div>
  <div class="metric"><span class="muted">JSON handoff</span><strong><a href="./llm_todo.json">Open llm_todo.json</a></strong></div>
</div>
<div class="panel">
  <h2>How to use this queue</h2>
  <p>Work from top to bottom. Saved fuzz crash artifacts come first, before any general cleanup. Preserve behavior unless tests and docs require otherwise. Every touched function should get updated tests, and network-facing code should also get stronger fuzz coverage.</p>
</div>
<div class="panel">
  <h2>Prioritized cleanup queue</h2>
  <table>
    <thead>
      <tr>
        <th>Rank</th>
        <th>Kind</th>
        <th>File</th>
        <th>Priority</th>
        <th>Why first</th>
        <th>Hot functions / debug</th>
        <th>Next action</th>
      </tr>
    </thead>
    <tbody>
$(($queueRows -join "`n"))
    </tbody>
  </table>
</div>
<div class="panel">
  <h2>Saved fuzz findings</h2>
$(if ($findingRows) {
@"
  <table>
    <thead>
      <tr>
        <th>Target</th>
        <th>Artifact</th>
        <th>Path</th>
        <th>Updated</th>
        <th>Bytes</th>
        <th>Expected scope</th>
        <th>SHA-256</th>
        <th>Hex preview</th>
        <th>ASCII preview</th>
        <th>Reproduce</th>
      </tr>
    </thead>
    <tbody>
$(($findingRows -join "`n"))
    </tbody>
  </table>
"@
} else {
@"
  <p>No saved fuzz findings are present under <code>server/fuzz/artifacts</code>.</p>
  <p class="muted">When fuzzing does produce crashes, they should be handled ahead of the general queue above.</p>
"@
})
</div>
<p class="footer"><a href="../index.html">Back to report index</a></p>
"@

        Write-ReportHtml -Path $reportPath -Title "Backend Hardening Queue" -Body $body
        Write-ReportHtml -Path $outputPath -Title "Backend Hardening Queue" -Body $body

        return [pscustomobject]@{
            Name = "Hardening Queue"
            Status = "ok"
            Notes = @($notes | Sort-Object -Unique)
            IndexPath = "hardening/index.html"
            ErrorMessage = $null
            Summary = [pscustomobject]@{
                Tasks = $queueItems.Count
                Findings = $findings.Count
            }
        }
    }
    catch {
        $errorMessage = $_.Exception.Message
        $notes.Add("Hardening queue generation failed: $errorMessage")
        $body = @"
<h1>Backend Hardening Queue Failed</h1>
<div class="panel">
  <p>The cleanup queue could not be generated.</p>
  <p><code>$(Escape-Html $errorMessage)</code></p>
</div>
<p class="footer"><a href="../index.html">Back to report index</a></p>
"@
        Write-ReportHtml -Path $reportPath -Title "Backend Hardening Queue Failed" -Body $body
        Write-ReportHtml -Path $outputPath -Title "Backend Hardening Queue Failed" -Body $body

        return [pscustomobject]@{
            Name = "Hardening Queue"
            Status = "failed"
            Notes = @($notes)
            IndexPath = "hardening/index.html"
            ErrorMessage = $errorMessage
        }
    }
}

function Invoke-DocsReport {
    $notes = [System.Collections.Generic.List[string]]::new()
    $reportPath = Join-Path $docsArtifactRoot "index.html"
    $outputPath = Join-Path $docsArtifactRoot "output.html"

    if (-not (Test-ToolAvailable -CommandName "mdbook")) {
        $notes.Add("Documentation artifacts were skipped because mdbook is not installed.")
        $body = @"
<h1>Documentation Artifacts Unavailable</h1>
<div class="panel">
  <p>mdbook is not installed, so the project docs site could not be generated.</p>
  <p class="muted">Install it with <code>./scripts/install-tools.ps1</code>.</p>
</div>
<p class="footer"><a href="../index.html">Back to report index</a></p>
"@
        Write-ReportHtml -Path $reportPath -Title "Documentation Artifacts Unavailable" -Body $body
        Write-ReportHtml -Path $outputPath -Title "Documentation Artifacts Unavailable" -Body $body

        return [pscustomobject]@{
            Name = "Docs"
            Status = "failed"
            Notes = @($notes)
            IndexPath = "docs/index.html"
            ErrorMessage = "mdbook is not installed."
        }
    }

    try {
        & (Join-Path $PSScriptRoot "build-docs.ps1")

        $docsSummaryPath = Join-Path $docsArtifactRoot "summary.json"
        $docEntries = @()
        if (Test-Path $docsSummaryPath) {
            $docEntries = @(
                Get-Content -Path $docsSummaryPath -Raw |
                    ConvertFrom-Json |
                    ForEach-Object { $_ }
            )
        }
        $publishedDocs = @($docEntries | Where-Object { $_.Published })
        $publicationPercent = if ($docEntries.Count -eq 0) {
            0.0
        }
        else {
            ($publishedDocs.Count / $docEntries.Count) * 100.0
        }
        $rustdocIndexPath = Join-Path $rustdocArtifactRoot "index.html"
        $rustdocOutputPath = Join-Path $rustdocArtifactRoot "output.html"
        $rustdocPublished = (Test-Path $rustdocIndexPath) -or (Test-Path $rustdocOutputPath)
        $scoreSummary = New-ScoreSummary `
            -Score (($publicationPercent * 0.85) + ($(if ($rustdocPublished) { 100.0 } else { 0.0 }) * 0.15)) `
            -Formula "85% Markdown publication coverage + 15% rustdoc availability" `
            -Breakdown @(
                "Published docs: $($publishedDocs.Count) / $($docEntries.Count)",
                "Publication coverage: $(Format-Percent -Value $publicationPercent)",
                "Rustdoc index: $(if ($rustdocPublished) { 'present' } else { 'missing' })"
            )

        $notes.Add("mdBook site generated from shared/docs and published under target/reports/docs/site.")
        $notes.Add("Rust API docs generated with cargo doc --workspace --all-features --no-deps and published under target/reports/rustdoc.")
        $notes.Add("The post-commit hook now regenerates the docs site and API docs alongside coverage, complexity, and callgraph artifacts.")

        return [pscustomobject]@{
            Name = "Docs"
            Status = "ok"
            Notes = @($notes)
            IndexPath = "docs/index.html"
            ErrorMessage = $null
            ScoreSummary = $scoreSummary
            Summary = [pscustomobject]@{
                PublicationPercent = $publicationPercent
                PublishedDocs = $publishedDocs.Count
                TotalDocs = $docEntries.Count
                RustdocPublished = $rustdocPublished
            }
        }
    }
    catch {
        $errorMessage = $_.Exception.Message
        $notes.Add("Documentation artifact generation failed: $errorMessage")
        $body = @"
<h1>Documentation Artifacts Failed</h1>
<div class="panel">
  <p>The documentation build step could not complete.</p>
  <p><code>$(Escape-Html $errorMessage)</code></p>
</div>
<p class="footer"><a href="../index.html">Back to report index</a></p>
"@
        Write-ReportHtml -Path $reportPath -Title "Documentation Artifacts Failed" -Body $body
        Write-ReportHtml -Path $outputPath -Title "Documentation Artifacts Failed" -Body $body

        return [pscustomobject]@{
            Name = "Docs"
            Status = "failed"
            Notes = @($notes)
            IndexPath = "docs/index.html"
            ErrorMessage = $errorMessage
        }
    }
}

function Invoke-FrontendReport {
    try {
        return & (Join-Path $PSScriptRoot "frontend-quality.ps1") -OutputRoot $frontendRoot
    }
    catch {
        $errorMessage = $_.Exception.Message
        $reportPath = Join-Path $frontendRoot "index.html"
        $outputPath = Join-Path $frontendRoot "output.html"
        $body = @"
<h1>Frontend Quality Report Failed</h1>
<div class="panel">
  <p>The frontend GDScript quality report could not complete.</p>
  <p><code>$(Escape-Html $errorMessage)</code></p>
</div>
<p class="footer"><a href="../index.html">Back to report index</a></p>
"@
        Write-ReportHtml -Path $reportPath -Title "Frontend Quality Report Failed" -Body $body
        Write-ReportHtml -Path $outputPath -Title "Frontend Quality Report Failed" -Body $body

        return [pscustomobject]@{
            Name = "Frontend Quality"
            Status = "failed"
            Notes = @("Frontend quality analysis failed: $errorMessage")
            IndexPath = "frontend/index.html"
            ErrorMessage = $errorMessage
        }
    }
}

function Invoke-ReportGeneration {
    $commitShort = Get-GitValue -CommandArgs @("rev-parse", "--short", "HEAD") -Fallback "unknown"
    $commitLong = Get-GitValue -CommandArgs @("rev-parse", "HEAD") -Fallback "unknown"
    $branchName = Get-GitValue -CommandArgs @("branch", "--show-current") -Fallback "detached"
    $generatedAt = (Get-Date).ToString("u")
    $sourceInventory = Get-SourceInventory

    New-Item -ItemType Directory -Force -Path $reportsRoot | Out-Null

    $results = @()
    switch ($Report) {
        "frontend" {
            $results += Invoke-FrontendReport
        }
        "coverage" {
            $results += Invoke-CoverageReport -SourceInventory $sourceInventory
        }
        "fuzz" {
            $results += Invoke-FuzzCoverageReport -SourceInventory $sourceInventory
        }
        "docs" {
            $results += Invoke-DocsReport
        }
        "callgraph" {
            $results += Invoke-CallgraphReport -SourceInventory $sourceInventory
        }
        "clean-code" {
            $results += Invoke-CleanCodeReport -SourceInventory $sourceInventory
        }
        "complexity" {
            $results += Invoke-ComplexityReport -SourceInventory $sourceInventory
        }
        "hardening" {
            $results += Invoke-FuzzCoverageReport -SourceInventory $sourceInventory
            $results += Invoke-CleanCodeReport -SourceInventory $sourceInventory
            $results += Invoke-ComplexityReport -SourceInventory $sourceInventory
            $results += Invoke-HardeningQueueReport -SourceInventory $sourceInventory
            $results += Invoke-FrontendReport
        }
        default {
            $results += Invoke-CoverageReport -SourceInventory $sourceInventory
            $results += Invoke-FuzzCoverageReport -SourceInventory $sourceInventory
            $results += Invoke-DocsReport
            $results += Invoke-CallgraphReport -SourceInventory $sourceInventory
            $results += Invoke-CleanCodeReport -SourceInventory $sourceInventory
            $results += Invoke-ComplexityReport -SourceInventory $sourceInventory
            $results += Invoke-HardeningQueueReport -SourceInventory $sourceInventory
            $results += Invoke-FrontendReport
        }
    }

    $scoredResults = @(
        $results |
            ForEach-Object {
                [pscustomobject]@{
                    Result = $_
                    ScoreSummary = Get-OptionalPropertyValue -InputObject $_ -PropertyName "ScoreSummary"
                }
            } |
            Where-Object { $null -ne $_.ScoreSummary }
    )
    $overallScoreSummary = if ($scoredResults.Count -gt 0) {
        $averageScore = [double](($scoredResults | ForEach-Object { $_.ScoreSummary.Score } | Measure-Object -Average).Average)
        New-ScoreSummary -Score $averageScore -Formula "Average of the generated scored reports" -Breakdown @()
    }
    else {
        $null
    }

    $cards = foreach ($result in $results) {
        $resultScoreSummary = Get-OptionalPropertyValue -InputObject $result -PropertyName "ScoreSummary"
        $scoreBlock = if ($null -ne $resultScoreSummary) {
            @"
  <p><strong>$(Format-Score -Score $resultScoreSummary.Score)</strong> $(Format-GradeBadge -Grade $resultScoreSummary.Grade)</p>
  <p class="muted">$(Escape-Html $resultScoreSummary.Formula)</p>
"@
        }
        else {
            @"
  <p><strong>Informational</strong></p>
"@
        }
        $breakdownBlock = if (($null -ne $resultScoreSummary) -and $resultScoreSummary.Breakdown) {
            "<p class=`"muted`">$(Escape-Html (($resultScoreSummary.Breakdown -join ' | ')))</p>"
        }
        else {
            ""
        }

        @"
<div class="metric">
  <span class="badge $(Get-StatusBadgeClass -Status $result.Status)">$(Escape-Html $result.Status.ToUpperInvariant())</span>
  <strong>$(Escape-Html $result.Name)</strong>
  $scoreBlock
  <p><a href="./$(Escape-Html $result.IndexPath)">Open $(Escape-Html $result.Name.ToLowerInvariant()) report</a></p>
  <p class="muted">$(Escape-Html $(if ($result.ErrorMessage) { $result.ErrorMessage } else { "Report generated successfully." }))</p>
  $breakdownBlock
</div>
"@
    }

    $notes = foreach ($result in $results) {
        foreach ($note in @($result.Notes)) {
            $note
        }
    }

    $noteItems = foreach ($note in ($notes | Sort-Object -Unique)) {
        "<li>$(Escape-Html $note)</li>"
    }

    $body = @"
<h1>Rarena Quality Reports</h1>
<p class="muted">Commit <code>$(Escape-Html $commitShort)</code> on <code>$(Escape-Html $branchName)</code>. Generated at $generatedAt.</p>
<div class="panel">
  <div class="grid">
    <div class="metric"><span class="muted">Overall quality score</span><strong>$(if ($null -ne $overallScoreSummary) { '{0} {1}' -f (Format-Score -Score $overallScoreSummary.Score), (Format-GradeBadge -Grade $overallScoreSummary.Grade) } else { 'n/a' })</strong></div>
    <div class="metric"><span class="muted">Commit</span><strong><code>$(Escape-Html $commitShort)</code></strong></div>
    <div class="metric"><span class="muted">Branch</span><strong>$(Escape-Html $branchName)</strong></div>
    <div class="metric"><span class="muted">Report root</span><strong><code>server/target/reports/output.html</code></strong></div>
    <div class="metric"><span class="muted">Full revision</span><strong><code>$(Escape-Html $commitLong)</code></strong></div>
  </div>
</div>
<div class="panel">
  <h2>Available reports</h2>
  <div class="grid">
$(($cards -join "`n"))
  </div>
</div>
<div class="panel">
  <h2>Known gaps and reasons</h2>
  <ul>
$(($noteItems -join "`n"))
  </ul>
</div>
"@

    Write-ReportHtml -Path (Join-Path $reportsRoot "index.html") -Title "Rarena Quality Reports" -Body $body
    Write-ReportHtml -Path (Join-Path $reportsRoot "output.html") -Title "Rarena Quality Reports" -Body $body
    New-Item -ItemType File -Force -Path (Join-Path $reportsRoot ".nojekyll") | Out-Null

    Write-Host "Reports written to $reportsRoot"

    $failed = @($results | Where-Object { $_.Status -eq "failed" })
    if ($FailOnCommandFailure -and $failed.Count -gt 0) {
        throw "One or more reports failed to generate. See target/reports/output.html for details."
    }
}

Invoke-ReportGeneration
