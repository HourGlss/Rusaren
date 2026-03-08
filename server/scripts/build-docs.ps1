[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"

$serverRoot = Split-Path -Parent $PSScriptRoot
$repoRoot = Split-Path -Parent $serverRoot
Set-Location $serverRoot

$reportsRoot = Join-Path $serverRoot "target\reports"
$docsRoot = Join-Path $reportsRoot "docs"
$docsSiteRoot = Join-Path $docsRoot "site"
$rustdocRoot = Join-Path $reportsRoot "rustdoc"
$docsBuildRoot = Join-Path $serverRoot "target\docs-build"
$bookRoot = Join-Path $docsBuildRoot "book"
$bookSourceRoot = Join-Path $bookRoot "src"
$rustdocBuildRoot = Join-Path $serverRoot "target\rustdoc-build"
$sharedDocsRoot = Join-Path $repoRoot "shared\docs"

function Escape-Html {
    param([AllowNull()][string]$Value)

    if ($null -eq $Value) {
        return ""
    }

    return [System.Net.WebUtility]::HtmlEncode($Value)
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

function Write-ArtifactHtml {
    param(
        [Parameter(Mandatory)]
        [string]$Path,
        [Parameter(Mandatory)]
        [string]$Title,
        [Parameter(Mandatory)]
        [string]$Body
    )

    $directory = Split-Path -Parent $Path
    if (-not (Test-Path $directory)) {
        New-Item -ItemType Directory -Force -Path $directory | Out-Null
    }

    $document = @"
<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>$Title</title>
<style>
body {
    margin: 0;
    font-family: "Segoe UI", Tahoma, Geneva, Verdana, sans-serif;
    background: linear-gradient(180deg, #f8f6f1 0%, #ece8dd 100%);
    color: #17202a;
}
main {
    max-width: 960px;
    margin: 0 auto;
    padding: 2rem 1.2rem 3rem;
}
.panel {
    background: #ffffff;
    border: 1px solid #d7dadd;
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
    background: #fbfbf9;
    border: 1px solid #d7dadd;
    border-radius: 14px;
    padding: 0.9rem 1rem;
}
.metric strong {
    display: block;
    margin-top: 0.35rem;
    font-size: 1.2rem;
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
a {
    color: #1d4ed8;
}
code {
    font-family: "Cascadia Code", Consolas, monospace;
}
table {
    width: 100%;
    border-collapse: collapse;
    margin-top: 1rem;
    font-size: 0.95rem;
}
th,
td {
    text-align: left;
    padding: 0.75rem;
    border-bottom: 1px solid #d7dadd;
    vertical-align: top;
}
th {
    background: #f8fafc;
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

    Set-Content -Path $Path -Value $document -Encoding UTF8
}

if (-not (Test-ToolAvailable -CommandName "mdbook")) {
    throw "mdbook is not installed. Install it with ./scripts/install-tools.ps1."
}

$orderedSources = @(
    "00_index.md",
    "01_principles.md",
    "02_repo_layout.md",
    "03_domain_model.md",
    "04_match_flow.md",
    "05_simulation_loop.md",
    "06_networking.md",
    "07_skills_spells_modifiers.md",
    "08_godot_client.md",
    "09_testing_ops.md",
    "10_maps.md",
    "11_classes.md",
    "12_rust_tooling.md",
    "13_verus_strategy.md",
    "14_buildability_assessment.md",
    "classes\warrior.md",
    "classes\rogue.md",
    "classes\mage.md",
    "classes\cleric.md",
    "maps\_template.md"
)
$orderedSourceSet = @{}
foreach ($relativePath in $orderedSources) {
    $orderedSourceSet[$relativePath.Replace('\', '/')] = $true
}

if (Test-Path $docsRoot) {
    Remove-Item -Recurse -Force -Path $docsRoot
}
if (Test-Path $rustdocRoot) {
    Remove-Item -Recurse -Force -Path $rustdocRoot
}
if (Test-Path $docsBuildRoot) {
    Remove-Item -Recurse -Force -Path $docsBuildRoot
}
if (Test-Path $rustdocBuildRoot) {
    Remove-Item -Recurse -Force -Path $rustdocBuildRoot
}

New-Item -ItemType Directory -Force -Path $bookSourceRoot | Out-Null
New-Item -ItemType Directory -Force -Path $docsRoot | Out-Null
New-Item -ItemType Directory -Force -Path $rustdocRoot | Out-Null

foreach ($relativePath in $orderedSources) {
    $sourcePath = Join-Path $sharedDocsRoot $relativePath
    if (-not (Test-Path $sourcePath)) {
        throw "Expected documentation source file was not found: $sourcePath"
    }

    $destinationPath = Join-Path $bookSourceRoot $relativePath
    $destinationDirectory = Split-Path -Parent $destinationPath
    if (-not (Test-Path $destinationDirectory)) {
        New-Item -ItemType Directory -Force -Path $destinationDirectory | Out-Null
    }

    Copy-Item -Path $sourcePath -Destination $destinationPath -Force
}

Copy-Item -Path (Join-Path $bookSourceRoot "00_index.md") -Destination (Join-Path $bookSourceRoot "index.md") -Force

$summaryContent = @"
# Summary

- [Overview](index.md)
- [Principles](01_principles.md)
- [Repo Layout](02_repo_layout.md)
- [Domain Model](03_domain_model.md)
- [Match Flow](04_match_flow.md)
- [Simulation Loop](05_simulation_loop.md)
- [Networking](06_networking.md)
- [Skills, Spells, and Modifiers](07_skills_spells_modifiers.md)
- [Godot Client](08_godot_client.md)
- [Testing, Validation, Ops](09_testing_ops.md)
- [Maps](10_maps.md)
  - [Map Template](maps/_template.md)
- [Classes](11_classes.md)
  - [Warrior](classes/warrior.md)
  - [Rogue](classes/rogue.md)
  - [Mage](classes/mage.md)
  - [Cleric](classes/cleric.md)
- [Rust Tooling](12_rust_tooling.md)
- [Verus Strategy](13_verus_strategy.md)
- [Buildability Assessment](14_buildability_assessment.md)
"@
Set-Content -Path (Join-Path $bookSourceRoot "SUMMARY.md") -Value $summaryContent -Encoding UTF8

$bookToml = @"
[book]
title = "Rusaren Docs"
description = "Architecture, protocol, gameplay, tooling, and operations docs for Rusaren."
language = "en"
src = "src"

[build]
build-dir = "../../reports/docs/site"

[output.html]
default-theme = "rust"
preferred-dark-theme = "navy"
git-repository-url = "https://github.com/HourGlss/Rusaren"
no-section-label = true

[output.html.fold]
enable = true
level = 1
"@
Set-Content -Path (Join-Path $bookRoot "book.toml") -Value $bookToml -Encoding UTF8

Invoke-CheckedCommand -Description "mdbook build" -Command {
    mdbook build $bookRoot | Out-Host
}

Invoke-CheckedCommand -Description "cargo doc" -Command {
    rustup run stable cargo doc --workspace --all-features --no-deps --target-dir $rustdocBuildRoot | Out-Host
}

Copy-Item -Path (Join-Path $rustdocBuildRoot "doc\*") -Destination $rustdocRoot -Recurse -Force

$rustdocEntries = @(
    Get-ChildItem -Path $rustdocRoot -Directory |
        Where-Object { Test-Path (Join-Path $_.FullName "index.html") } |
        Sort-Object Name |
        ForEach-Object {
            [pscustomobject]@{
                Name = $_.Name
                RelativeIndex = "./$($_.Name)/index.html"
            }
        }
)

$docEntries = @()
foreach ($docFile in Get-ChildItem -Path $sharedDocsRoot -Recurse -File -Filter *.md | Sort-Object FullName) {
    $relativePath = $docFile.FullName.Substring($sharedDocsRoot.Length).TrimStart('\', '/')
    $relativePath = $relativePath.Replace('\', '/')
    $lines = @(Get-Content -Path $docFile.FullName)
    $nonEmptyLineCount = @($lines | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }).Count
    $wordCount = ([regex]::Matches((Get-Content -Path $docFile.FullName -Raw), '\S+')).Count
    $included = $orderedSourceSet.ContainsKey($relativePath)
    $outputRelative = $null
    if ($included) {
        $outputRelative = if ($relativePath -eq "00_index.md") {
            "index.html"
        }
        else {
            ($relativePath -replace '\.md$', '.html')
        }
    }

    $published = $included -and (Test-Path (Join-Path $docsSiteRoot $outputRelative))
    $docEntries += [pscustomobject]@{
        SourcePath = $relativePath
        LineCount = $lines.Count
        NonEmptyLineCount = $nonEmptyLineCount
        WordCount = $wordCount
        Included = $included
        Published = $published
        OutputPath = if ($published) { $outputRelative } else { $null }
        Note = if ($published) {
            $null
        }
        elseif ($included) {
            "Included in SUMMARY, but the HTML output was not found."
        }
        else {
            "Not included in the mdBook SUMMARY."
        }
    }
}

$docEntries | ConvertTo-Json -Depth 4 | Set-Content -Path (Join-Path $docsRoot "summary.json") -Encoding UTF8

$commitShort = "unknown"
try {
    $commitShort = (git rev-parse --short HEAD 2>$null | Select-Object -First 1).Trim()
}
catch {
    $commitShort = "unknown"
}

$publishedDocs = @($docEntries | Where-Object { $_.Published })
$missingDocs = @($docEntries | Where-Object { -not $_.Published })
$coveragePercent = if ($docEntries.Count -eq 0) {
    0
}
else {
    [math]::Round(($publishedDocs.Count / $docEntries.Count) * 100, 2)
}
$rustdocPublished = $rustdocEntries.Count -gt 0
$docsScore = [math]::Round(($coveragePercent * 0.85) + ($(if ($rustdocPublished) { 100.0 } else { 0.0 }) * 0.15), 2)
$docsGrade = if ($docsScore -ge 90) {
    "A"
}
elseif ($docsScore -ge 80) {
    "B"
}
elseif ($docsScore -ge 70) {
    "C"
}
elseif ($docsScore -ge 60) {
    "D"
}
elseif ($docsScore -ge 50) {
    "E"
}
else {
    "F"
}
$docRows = foreach ($entry in $docEntries) {
    $statusLabel = if ($entry.Published) { "Published" } else { "Missing" }
    $statusClass = if ($entry.Published) { "badge-ok" } else { "badge-warn" }
    $outputCell = if ($entry.OutputPath) {
        '<a href="./site/{0}"><code>{0}</code></a>' -f (Escape-Html $entry.OutputPath)
    }
    else {
        '<span style="color: #6b7280;">Not published</span>'
    }
    $noteCell = if ([string]::IsNullOrWhiteSpace([string]$entry.Note)) {
        ""
    }
    else {
        Escape-Html $entry.Note
    }

@"
<tr>
  <td><code>$(Escape-Html $entry.SourcePath)</code></td>
  <td><span class="badge $statusClass">$statusLabel</span></td>
  <td>$outputCell</td>
  <td>$($entry.LineCount)</td>
  <td>$($entry.NonEmptyLineCount)</td>
  <td>$($entry.WordCount)</td>
  <td>$noteCell</td>
</tr>
"@
}

$docsBody = @"
<h1>Rusaren Documentation</h1>
<p>Documentation artifacts generated from <code>shared/docs</code> via <code>mdBook</code> and from the Rust workspace via <code>cargo doc --workspace --all-features --no-deps</code>.</p>
<div class="grid">
  <div class="metric">
    <span>Docs site</span>
    <strong><a href="./site/index.html">Open mdBook site</a></strong>
  </div>
  <div class="metric">
    <span>Docs score</span>
    <strong>$docsScore/100 ($docsGrade)</strong>
  </div>
  <div class="metric">
    <span>API docs</span>
    <strong><a href="../rustdoc/index.html">Open rustdoc</a></strong>
  </div>
  <div class="metric">
    <span>Commit</span>
    <strong><code>$commitShort</code></strong>
  </div>
  <div class="metric">
    <span>Docs published</span>
    <strong>$($publishedDocs.Count) / $($docEntries.Count)</strong>
  </div>
  <div class="metric">
    <span>Per-file docs coverage</span>
    <strong>$coveragePercent%</strong>
  </div>
</div>
<div class="panel">
  <h2>Source of truth</h2>
  <p>The authored project documentation remains under <code>shared/docs</code>. This site is a generated view intended for local review and CI artifacts.</p>
  <p>Score formula: 85% Markdown publication coverage plus 15% rustdoc availability.</p>
</div>
<div class="panel">
  <h2>Per-file docs inventory</h2>
  <p>Coverage here means publication coverage: each Markdown file under <code>shared/docs</code> is checked for inclusion in the generated <code>mdBook</code> site.</p>
  <table>
    <thead>
      <tr>
        <th>Source file</th>
        <th>Status</th>
        <th>Published output</th>
        <th>Lines</th>
        <th>Non-empty lines</th>
        <th>Words</th>
        <th>Note</th>
      </tr>
    </thead>
    <tbody>
$(($docRows -join "`n"))
    </tbody>
  </table>
</div>
"@

Write-ArtifactHtml -Path (Join-Path $docsRoot "index.html") -Title "Rusaren Documentation" -Body $docsBody
Write-ArtifactHtml -Path (Join-Path $docsRoot "output.html") -Title "Rusaren Documentation" -Body $docsBody

if ($rustdocEntries.Count -gt 0) {
    $rustdocEntryItems = foreach ($entry in $rustdocEntries) {
        "<li><a href=`"$(Escape-Html $entry.RelativeIndex)`"><code>$(Escape-Html $entry.Name)</code></a></li>"
    }
    $rustdocListBody = @"
<p>Available crate and binary documentation:</p>
<ul>
$(($rustdocEntryItems -join "`n"))
</ul>
"@
}
else {
    $rustdocListBody = '<p>No rustdoc crate indexes were generated.</p>'
}

$rustdocBody = @"
<h1>Rusaren Rust API Docs</h1>
<p>Workspace API documentation generated with <code>cargo doc --workspace --all-features --no-deps</code>.</p>
<div class="panel">
  $rustdocListBody
</div>
"@

Write-ArtifactHtml -Path (Join-Path $rustdocRoot "index.html") -Title "Rusaren Rust API Docs" -Body $rustdocBody
Write-ArtifactHtml -Path (Join-Path $rustdocRoot "output.html") -Title "Rusaren Rust API Docs" -Body $rustdocBody

Write-Host "Documentation artifacts written to $docsRoot and $rustdocRoot"
