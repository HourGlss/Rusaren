[CmdletBinding()]
param(
    [ValidateSet("all", "coverage", "complexity", "callgraph", "docs", "fuzz", "hardening")]
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
$callgraphRoot = Join-Path $reportsRoot "callgraph"
$docsArtifactRoot = Join-Path $reportsRoot "docs"
$rustdocArtifactRoot = Join-Path $reportsRoot "rustdoc"
$hardeningRoot = Join-Path $reportsRoot "hardening"

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
    return $normalizedPath -like "crates/*/src/*.rs"
}

function Test-IsTestSourcePath {
    param([string]$Path)

    $normalizedPath = Get-NormalizedDisplayPath -Path $Path
    return $normalizedPath -like "crates/*/tests/*.rs"
}

function Test-IsEntryPointSourcePath {
    param([string]$Path)

    $normalizedPath = Get-NormalizedDisplayPath -Path $Path
    return $normalizedPath -eq "bin/dedicated_server/src/main.rs"
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
            $isRuntimeSource = Test-IsRuntimeSourcePath -Path $displayPath
            $isTestSource = Test-IsTestSourcePath -Path $displayPath
            $isEntryPointSource = Test-IsEntryPointSourcePath -Path $displayPath
            $isToolingSource = Test-IsToolingSourcePath -Path $displayPath

            $inventory[$displayPath] = [pscustomobject]@{
                DisplayPath = $displayPath
                NormalizedPath = $normalizedPath
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
            Scope = "crates/game_net/src/lib.rs"
            Description = "Packet framing and header validation."
            Paths = @("crates/game_net/src/lib.rs")
            Primary = $true
        },
        [pscustomobject]@{
            Target = "control_command_decode"
            Scope = "crates/game_net/src/control.rs plus game_domain validation via decoded identifiers and names."
            Description = "Control command decode and validation."
            Paths = @("crates/game_net/src/control.rs")
            Primary = $true
        },
        [pscustomobject]@{
            Target = "input_frame_decode"
            Scope = "crates/game_net/src/lib.rs"
            Description = "Input packet decode and button/context validation."
            Paths = @("crates/game_net/src/lib.rs")
            Primary = $true
        },
        [pscustomobject]@{
            Target = "session_ingress"
            Scope = "crates/game_net/src/ingress.rs plus control decode and domain validation."
            Description = "Session binding and hostile ingress sequencing."
            Paths = @("crates/game_net/src/ingress.rs", "crates/game_net/src/control.rs")
            Primary = $true
        },
        [pscustomobject]@{
            Target = "server_control_event_decode"
            Scope = "crates/game_net/src/control.rs plus game_domain validation via decoded lobby snapshots and records."
            Description = "Server control event decode for lobby directory and full lobby snapshot payloads."
            Paths = @("crates/game_net/src/control.rs")
            Primary = $true
        },
        [pscustomobject]@{
            Target = "webrtc_signal_message_parse"
            Scope = "crates/game_api/src/webrtc.rs via websocket signaling JSON validation."
            Description = "WebRTC signaling message decode and validation."
            Paths = @("crates/game_api/src/webrtc.rs")
            Primary = $true
        },
        [pscustomobject]@{
            Target = "control_command_roundtrip"
            Scope = "crates/game_net/src/control.rs via structured encode/decode differential fuzzing."
            Description = "Structured round-trip fuzzing for valid control command packets."
            Paths = @("crates/game_net/src/control.rs")
            Primary = $false
        },
        [pscustomobject]@{
            Target = "input_frame_roundtrip"
            Scope = "crates/game_net/src/lib.rs via structured encode/decode differential fuzzing."
            Description = "Structured round-trip fuzzing for valid player input packets."
            Paths = @("crates/game_net/src/lib.rs")
            Primary = $false
        },
        [pscustomobject]@{
            Target = "webrtc_signal_message_roundtrip"
            Scope = "crates/game_api/src/webrtc.rs via structured JSON round-trip fuzzing."
            Description = "Structured round-trip fuzzing for valid signaling messages."
            Paths = @("crates/game_api/src/webrtc.rs")
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

function Get-FuzzCorpusStats {
    param(
        [string]$ServerRoot,
        [string]$Target
    )

    $seedDir = Join-Path $ServerRoot ("fuzz\corpus\" + $Target)
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
        $seedHashes[(Get-FileHash -Algorithm SHA256 -Path $seedFile.FullName).Hash] = $true
    }

    $discoveredFiles = @(
        foreach ($generatedFile in $generatedFiles) {
            $hash = (Get-FileHash -Algorithm SHA256 -Path $generatedFile.FullName).Hash
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
                $sha256 = (Get-FileHash -Algorithm SHA256 -Path $_.FullName).Hash

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
                NormalizedPath = $displayPath -replace '\\', '/'
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
        $runtimeRegionCovered = 0
        $runtimeRegionTotal = 0
        foreach ($runtimeFile in $runtimeFiles) {
            $coverageFile = @($coverageData.files | Where-Object {
                (Convert-ToDisplayPath -Path $_.filename) -eq $runtimeFile.DisplayPath
            } | Select-Object -First 1)
            if ($coverageFile.Count -eq 0) {
                continue
            }

            $runtimeRegionCovered += [int]$coverageFile[0].summary.regions.covered
            $runtimeRegionTotal += [int]$coverageFile[0].summary.regions.count
        }
        $runtimeRegionPercent = if ($runtimeRegionTotal -gt 0) {
            ([double]$runtimeRegionCovered / [double]$runtimeRegionTotal) * 100.0
        }
        else {
            0.0
        }
        $scoreSummary = New-ScoreSummary `
            -Score (($runtimeLinePercent * 0.5) + ($runtimeFunctionPercent * 0.3) + ($runtimeRegionPercent * 0.2)) `
            -Formula "50% runtime line + 30% runtime function + 20% runtime region coverage" `
            -Breakdown @(
                "Runtime lines: $(Format-Percent -Value $runtimeLinePercent)",
                "Runtime functions: $(Format-Percent -Value $runtimeFunctionPercent)",
                "Runtime regions: $(Format-Percent -Value $runtimeRegionPercent)"
            )
        $notes.Add("Doctests are validated separately by ./scripts/quality.ps1 doc but are not included here because stable doctest coverage is still unavailable in this workflow.")
        $notes.Add("Headline coverage scoring is scoped to backend runtime source files under crates/*/src/*.rs.")
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
  <div class="metric"><span class="muted">Runtime region coverage</span><strong>$(Format-Percent -Value $runtimeRegionPercent)</strong></div>
  <div class="metric"><span class="muted">Scored runtime files</span><strong>$($runtimeFiles.Count)</strong></div>
  <div class="metric"><span class="muted">Execution mode</span><strong>$(if ($usedNextest) { "cargo llvm-cov nextest" } else { "cargo llvm-cov test" })</strong></div>
</div>
<div class="panel">
  <p class="muted">The headline score is based on backend runtime source files. The table below still includes tooling and test files emitted by the coverage export.</p>
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
            ScoreSummary = $scoreSummary
            Summary = [pscustomobject]@{
                Lines = $runtimeLinePercent
                Functions = $runtimeFunctionPercent
                Regions = $runtimeRegionPercent
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
        foreach ($source in @($SourceInventory.Values | Where-Object { $_.IsRuntimeSource })) {
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
        $manageableRuntimePercent = if ($filesWithFunctions.Count -eq 0) {
            0.0
        }
        else {
            ($manageableFiles.Count / $filesWithFunctions.Count) * 100.0
        }
        $scoreSummary = New-ScoreSummary `
            -Score (($averageWorstFunctionScore * 0.5) + ($averageFunctionScore * 0.3) + ($manageableRuntimePercent * 0.2)) `
            -Formula "50% average runtime worst-function grade + 30% average runtime per-file function grade + 20% runtime files without E/F hotspots" `
            -Breakdown @(
                "Average runtime worst-function grade score: $("{0:N2}" -f $averageWorstFunctionScore)",
                "Average runtime function grade score: $("{0:N2}" -f $averageFunctionScore)",
                "Runtime files without E/F hotspots: $(Format-Percent -Value $manageableRuntimePercent)"
            )

        $notes.Add("Headline complexity scoring is scoped to backend runtime source files under crates/*/src/*.rs.")
        $notes.Add("Entrypoints, tooling binaries, and integration tests stay visible in the supplemental table but do not affect the headline complexity score.")
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
  <div class="metric"><span class="muted">Scored runtime files</span><strong>$($scoredFileMetrics.Count)</strong></div>
  <div class="metric"><span class="muted">Supplemental files</span><strong>$($supplementalFileMetrics.Count)</strong></div>
  <div class="metric"><span class="muted">Tracked runtime functions</span><strong>$($scoredFunctionMetrics.Count)</strong></div>
  <div class="metric"><span class="muted">Worst function CC</span><strong>$(if ($null -ne $worstFunction) { '{0} ({1:N0})' -f $worstFunction.CyclomaticGrade, $worstFunction.Cyclomatic } else { 'n/a' })</strong></div>
  <div class="metric"><span class="muted">Worst file avg CC</span><strong>$(if (($null -ne $worstFile) -and ($null -ne $worstFile.AverageFunctionCyclomatic)) { '{0} ({1:N2})' -f $worstFile.AverageFunctionGrade, $worstFile.AverageFunctionCyclomatic } else { 'n/a' })</strong></div>
  <div class="metric"><span class="muted">Runtime files without E/F</span><strong>$(Format-Percent -Value $manageableRuntimePercent)</strong></div>
</div>
<div class="panel">
  <h2>Grade scale</h2>
  <p><strong>Cyclomatic:</strong> A 1-5, B 6-10, C 11-20, D 21-30, E 31-40, F 41+.</p>
  <p><strong>Maintainability:</strong> A &gt;19, B 10-19, C &lt;=9.</p>
  <p class="muted">The headline score is based on runtime function health, while raw MI stays visible as a supporting signal.</p>
</div>
<div class="panel">
  <h2>Scored backend runtime files</h2>
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
  <h2>Top runtime function hotspots</h2>
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
    $coreFallbackSvgPath = Join-Path $callgraphRoot "backend_core.simple.svg"
    $overviewFallbackSvgPath = Join-Path $callgraphRoot "backend_core.overview.simple.svg"
    $summaryJsonPath = Join-Path $callgraphRoot "backend_core.summary.json"
    $backendCoreFiles = @(
        "crates/game_api/src/app.rs",
        "crates/game_api/src/realtime.rs",
        "crates/game_api/src/transport.rs",
        "crates/game_domain/src/lib.rs",
        "crates/game_net/src/lib.rs",
        "crates/game_net/src/control.rs",
        "crates/game_net/src/ingress.rs",
        "crates/game_lobby/src/lib.rs",
        "crates/game_match/src/lib.rs",
        "crates/game_sim/src/lib.rs"
    )
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
            if (Test-Path "index.scip") {
                Remove-Item -Force -Path "index.scip"
            }

            $scipIndexerWarning = $null
            try {
                & rust-analyzer scip . | Out-Host
                if ($LASTEXITCODE -ne 0) {
                    throw "rust-analyzer scip failed with exit code $LASTEXITCODE."
                }
            }
            catch {
                if (Test-Path "index.scip") {
                    $scipIndexerWarning = $_.Exception.Message
                }
                else {
                    throw
                }
            }

            if (-not (Test-Path "index.scip")) {
                throw "rust-analyzer did not produce index.scip in the server workspace root."
            }

            Move-Item -Force -Path "index.scip" -Destination $fullScipPath
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

        $backendCoreFiles = @(
            "crates/game_api/src/app.rs",
            "crates/game_api/src/realtime.rs",
            "crates/game_api/src/records.rs",
            "crates/game_api/src/transport.rs",
            "crates/game_domain/src/lib.rs",
            "crates/game_lobby/src/lib.rs",
            "crates/game_match/src/lib.rs",
            "crates/game_net/src/control.rs",
            "crates/game_net/src/ingress.rs",
            "crates/game_net/src/lib.rs",
            "crates/game_sim/src/lib.rs"
        )
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
                HotFunctions = if ($null -ne $functionSummary) { @($functionSummary.HotFunctions) } else { @() }
                Reasons = @($reasons)
                Actions = @($actions)
                Prompt = Get-BackendCorePrompt -FilePath $displayPath -HotFunctions @($functionSummary.HotFunctions) -FuzzLinePercent $fuzzLinePercent -FuzzFunctionPercent $fuzzFunctionPercent
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
            $hotspotLabel = if ($task.Kind -eq "crash_finding") {
                "Target: $(Escape-Html $task.Finding.Target)<br />Scope: $(Escape-Html $task.Finding.Scope)<br />SHA-256: <code>$(Escape-Html $task.Finding.Sha256)</code><br />Preview: <code>$(Escape-Html $task.Finding.HexPreview)</code>"
            }
            elseif ($task.HotFunctions.Count -gt 0) {
                ($task.HotFunctions | ForEach-Object { "$($_.Name) (CC $([int]$_.Cyclomatic))" }) -join "<br />"
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
            elseif ($task.HotFunctions.Count -gt 0) {
                foreach ($function in $task.HotFunctions) {
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
                            $_.HotFunctions | ForEach-Object {
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
        "fuzz" {
            $results += Invoke-FuzzCoverageReport -SourceInventory $sourceInventory
        }
        "docs" {
            $results += Invoke-DocsReport
        }
        "callgraph" {
            $results += Invoke-CallgraphReport -SourceInventory $sourceInventory
        }
        "complexity" {
            $results += Invoke-ComplexityReport -SourceInventory $sourceInventory
        }
        "hardening" {
            $results += Invoke-FuzzCoverageReport -SourceInventory $sourceInventory
            $results += Invoke-ComplexityReport -SourceInventory $sourceInventory
            $results += Invoke-HardeningQueueReport -SourceInventory $sourceInventory
        }
        default {
            $results += Invoke-CoverageReport -SourceInventory $sourceInventory
            $results += Invoke-FuzzCoverageReport -SourceInventory $sourceInventory
            $results += Invoke-DocsReport
            $results += Invoke-CallgraphReport -SourceInventory $sourceInventory
            $results += Invoke-ComplexityReport -SourceInventory $sourceInventory
            $results += Invoke-HardeningQueueReport -SourceInventory $sourceInventory
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
        New-ScoreSummary -Score $averageScore -Formula "Average of Coverage, Fuzzing, Docs, and Complexity scores" -Breakdown @()
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
