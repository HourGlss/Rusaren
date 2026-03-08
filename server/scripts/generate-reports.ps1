[CmdletBinding()]
param(
    [ValidateSet("all", "coverage", "complexity")]
    [string]$Report = "all",
    [switch]$FailOnCommandFailure
)

$ErrorActionPreference = "Stop"

$serverRoot = Split-Path -Parent $PSScriptRoot
$repoRoot = Split-Path -Parent $serverRoot
Set-Location $serverRoot

$cargoBin = Join-Path $HOME ".cargo\bin"
if (Test-Path $cargoBin) {
    $env:PATH = "$cargoBin;$env:PATH"
}

$reportsRoot = Join-Path $serverRoot "target\reports"
$coverageRoot = Join-Path $reportsRoot "coverage"
$complexityRoot = Join-Path $reportsRoot "complexity"

function Escape-Html {
    param([AllowNull()][string]$Value)

    if ($null -eq $Value) {
        return ""
    }

    return [System.Net.WebUtility]::HtmlEncode($Value)
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

function Get-StatusBadgeClass {
    param([string]$Status)

    switch ($Status) {
        "ok" { return "badge-ok" }
        "warning" { return "badge-warn" }
        default { return "badge-bad" }
    }
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

            $inventory[$displayPath] = [pscustomobject]@{
                DisplayPath = $displayPath
                IsPlaceholder = $isPlaceholder
                HasInlineTests = $hasInlineTests
                Reason = if ($isPlaceholder) {
                    "Only crate-level docs and attributes exist here; there is no substantive executable logic to cover yet."
                }
                elseif (-not $hasInlineTests) {
                    "No inline #[test] functions exist in this file yet."
                }
                else {
                    $null
                }
            }
        }
    }

    return $inventory
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
    if (-not $usedNextest) {
        $notes.Add("cargo-nextest is not installed; coverage fell back to cargo test.")
    }

    try {
        Invoke-CheckedCommand -Description "cargo llvm-cov clean" -Command {
            rustup run stable cargo llvm-cov clean --workspace | Out-Host
        }
        if (Test-Path $coverageRoot) {
            Remove-Item -Recurse -Force -Path $coverageRoot
        }

        New-Item -ItemType Directory -Force -Path $coverageRoot | Out-Null

        if ($usedNextest) {
            Invoke-CheckedCommand -Description "cargo llvm-cov nextest" -Command {
                rustup run stable cargo llvm-cov nextest --workspace --all-features --no-report | Out-Host
            }
        }
        else {
            Invoke-CheckedCommand -Description "cargo llvm-cov test" -Command {
                rustup run stable cargo llvm-cov --workspace --all-features --no-report | Out-Host
            }
        }

        Invoke-CheckedCommand -Description "cargo llvm-cov json report" -Command {
            rustup run stable cargo llvm-cov report --json --summary-only --output-path $summaryPath | Out-Host
        }
        Invoke-CheckedCommand -Description "cargo llvm-cov html report" -Command {
            rustup run stable cargo llvm-cov report --html --output-dir $coverageRoot | Out-Host
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
                LinePercent = [double]$file.summary.lines.percent
                FunctionPercent = [double]$file.summary.functions.percent
                RegionPercent = [double]$file.summary.regions.percent
                CoveredLines = [int]$file.summary.lines.covered
                TotalLines = [int]$file.summary.lines.count
                CoveredFunctions = [int]$file.summary.functions.covered
                TotalFunctions = [int]$file.summary.functions.count
            }
        }

        $files = @($files | Sort-Object LinePercent, DisplayPath)
        $totals = $coverageData.totals
        $notes.Add("Doctests are validated separately by ./scripts/quality.ps1 doc but are not included here because stable doctest coverage is still unavailable in this workflow.")
        $notes.Add("Browser, Godot, and live WebRTC integration coverage do not exist yet because the frontend client and transport adapter have not been implemented.")

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
  <div class="metric"><span class="muted">Line coverage</span><strong>$(Format-Percent -Value ([double]$totals.lines.percent))</strong></div>
  <div class="metric"><span class="muted">Function coverage</span><strong>$(Format-Percent -Value ([double]$totals.functions.percent))</strong></div>
  <div class="metric"><span class="muted">Region coverage</span><strong>$(Format-Percent -Value ([double]$totals.regions.percent))</strong></div>
  <div class="metric"><span class="muted">Execution mode</span><strong>$(if ($usedNextest) { "cargo llvm-cov nextest" } else { "cargo llvm-cov test" })</strong></div>
</div>
<div class="panel">
  <h2>Per-file summary</h2>
  <table>
    <thead>
      <tr>
        <th>File</th>
        <th>Lines</th>
        <th>Covered lines</th>
        <th>Functions</th>
        <th>Covered functions</th>
        <th>Regions</th>
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
            Summary = [pscustomobject]@{
                Lines = [double]$totals.lines.percent
                Functions = [double]$totals.functions.percent
                Regions = [double]$totals.regions.percent
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
                Cyclomatic = [double]$metrics.metrics.cyclomatic.sum
                Cognitive = [double]$metrics.metrics.cognitive.sum
                FunctionCount = [int]$metrics.metrics.nom.functions
                Sloc = [double]$metrics.metrics.loc.sloc
            }

            foreach ($node in @($metrics.spaces)) {
                $functionMetrics += Get-ComplexityFunctions -Node $node -FilePath $displayPath
            }
        }

        $fileMetrics = @($fileMetrics | Sort-Object Mi, DisplayPath)
        $functionMetrics = @($functionMetrics | Sort-Object @{ Expression = "Cognitive"; Descending = $true }, @{ Expression = "Cyclomatic"; Descending = $true }, FilePath, Name)

        $notes.Add("These metrics include inline test modules because the analyzer works on whole Rust source files.")
        $notes.Add("Placeholder crates with only crate-level docs and attributes have no meaningful complexity data yet.")

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

        $fileRows = foreach ($file in $fileMetrics) {
            @"
<tr>
  <td><code>$(Escape-Html $file.DisplayPath)</code></td>
  <td>$("{0:N2}" -f $file.Mi)</td>
  <td>$("{0:N0}" -f $file.Cyclomatic)</td>
  <td>$("{0:N0}" -f $file.Cognitive)</td>
  <td>$("{0:N0}" -f $file.FunctionCount)</td>
  <td>$("{0:N0}" -f $file.Sloc)</td>
</tr>
"@
        }

        $hotspotRows = foreach ($function in ($functionMetrics | Select-Object -First 15)) {
            @"
<tr>
  <td><code>$(Escape-Html $function.FilePath)</code></td>
  <td><code>$(Escape-Html $function.Name)</code></td>
  <td>$("{0:N0}" -f $function.Cognitive)</td>
  <td>$("{0:N0}" -f $function.Cyclomatic)</td>
  <td>$("{0:N2}" -f $function.Mi)</td>
  <td>$($function.StartLine)-$($function.EndLine)</td>
</tr>
"@
        }

        $noteItems = foreach ($note in ($notes | Sort-Object -Unique)) {
            "<li>$(Escape-Html $note)</li>"
        }

        $body = @"
<h1>Complexity Report</h1>
<p class="muted">Commit <code>$(Escape-Html (Get-GitValue -CommandArgs @("rev-parse", "--short", "HEAD") -Fallback "unknown"))</code>. Raw analyzer output lives under <code>target/reports/complexity/data</code>.</p>
<div class="grid">
  <div class="metric"><span class="muted">Analyzed files</span><strong>$($fileMetrics.Count)</strong></div>
  <div class="metric"><span class="muted">Tracked functions</span><strong>$($functionMetrics.Count)</strong></div>
  <div class="metric"><span class="muted">Lowest file MI</span><strong>$(if ($fileMetrics.Count -gt 0) { "{0:N2}" -f ($fileMetrics[0].Mi) } else { "n/a" })</strong></div>
  <div class="metric"><span class="muted">Highest function cognitive score</span><strong>$(if ($functionMetrics.Count -gt 0) { "{0:N0}" -f ($functionMetrics[0].Cognitive) } else { "n/a" })</strong></div>
</div>
<div class="panel">
  <h2>Per-file metrics</h2>
  <table>
    <thead>
      <tr>
        <th>File</th>
        <th>MI (VS)</th>
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
  <h2>Top function hotspots</h2>
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

        return [pscustomobject]@{
            Name = "Complexity"
            Status = "ok"
            Notes = @($notes | Sort-Object -Unique)
            IndexPath = "complexity/index.html"
            ErrorMessage = $null
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

function Invoke-ReportGeneration {
    $commitShort = Get-GitValue -CommandArgs @("rev-parse", "--short", "HEAD") -Fallback "unknown"
    $commitLong = Get-GitValue -CommandArgs @("rev-parse", "HEAD") -Fallback "unknown"
    $branchName = Get-GitValue -CommandArgs @("branch", "--show-current") -Fallback "detached"
    $generatedAt = (Get-Date).ToString("u")
    $sourceInventory = Get-SourceInventory

    if (Test-Path $reportsRoot) {
        Remove-Item -Recurse -Force -Path $reportsRoot
    }

    New-Item -ItemType Directory -Force -Path $reportsRoot | Out-Null

    $results = @()
    switch ($Report) {
        "coverage" {
            $results += Invoke-CoverageReport -SourceInventory $sourceInventory
        }
        "complexity" {
            $results += Invoke-ComplexityReport -SourceInventory $sourceInventory
        }
        default {
            $results += Invoke-CoverageReport -SourceInventory $sourceInventory
            $results += Invoke-ComplexityReport -SourceInventory $sourceInventory
        }
    }

    $cards = foreach ($result in $results) {
        @"
<div class="metric">
  <span class="badge $(Get-StatusBadgeClass -Status $result.Status)">$(Escape-Html $result.Status.ToUpperInvariant())</span>
  <strong>$(Escape-Html $result.Name)</strong>
  <p><a href="./$(Escape-Html $result.IndexPath)">Open $(Escape-Html $result.Name.ToLowerInvariant()) report</a></p>
  <p class="muted">$(Escape-Html $(if ($result.ErrorMessage) { $result.ErrorMessage } else { "Report generated successfully." }))</p>
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

    Write-Host "Reports written to $reportsRoot"

    $failed = @($results | Where-Object { $_.Status -eq "failed" })
    if ($FailOnCommandFailure -and $failed.Count -gt 0) {
        throw "One or more reports failed to generate. See target/reports/output.html for details."
    }
}

Invoke-ReportGeneration
