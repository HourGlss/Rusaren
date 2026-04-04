[CmdletBinding()]
param(
    [string]$OutputRoot = ""
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
if ($PSVersionTable.PSVersion.Major -ge 7) {
    $PSNativeCommandUseErrorActionPreference = $true
}

$serverRoot = Split-Path -Parent $PSScriptRoot
$repoRoot = Split-Path -Parent $serverRoot
if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
    $OutputRoot = Join-Path $serverRoot "target\reports\frontend"
}

$runtimeScriptsRoot = Join-Path $repoRoot "client\godot\scripts"
$runtimeMetricsPath = Join-Path $OutputRoot "runtime_monitors.json"
$godotDocsVersion = "4.6 stable"

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
body{margin:0;font-family:"Segoe UI",Tahoma,Geneva,Verdana,sans-serif;background:linear-gradient(180deg,#fbfaf7 0%,#f1efe9 100%);color:#17202a}
main{max-width:1240px;margin:0 auto;padding:2rem 1.5rem 3rem}
.panel{background:#fff;border:1px solid #d6d9dc;border-radius:18px;padding:1rem 1.2rem;margin:1rem 0;box-shadow:0 10px 30px rgba(17,24,39,.05)}
.grid{display:grid;gap:1rem;grid-template-columns:repeat(auto-fit,minmax(220px,1fr))}
.metric{background:#fcfcfb;border:1px solid #d6d9dc;border-radius:14px;padding:.9rem 1rem}
.metric strong{display:block;font-size:1.4rem;margin-top:.35rem}
.metric .detail,.muted{color:#5f6b76}
.badge{display:inline-block;border-radius:999px;padding:.2rem .65rem;font-size:.85rem;font-weight:700;letter-spacing:.02em}
.badge-grade-a{background:#dcfce7;color:#166534}.badge-grade-b{background:#d1fae5;color:#065f46}.badge-grade-c{background:#fef3c7;color:#92400e}.badge-grade-d{background:#fed7aa;color:#9a3412}.badge-grade-e{background:#fecaca;color:#b91c1c}.badge-grade-f{background:#fee2e2;color:#991b1b}.badge-warn{background:#fef3c7;color:#92400e}
table{width:100%;border-collapse:collapse;margin-top:1rem;font-size:.96rem}th,td{text-align:left;padding:.75rem;border-bottom:1px solid #d6d9dc;vertical-align:top}th{background:#f8fafc}
a{color:#1d4ed8}code{font-family:"Cascadia Code",Consolas,monospace;font-size:.92em}.footer{margin-top:2rem;font-size:.92rem}
</style>
</head>
<body><main>$Body</main></body></html>
"@

    $directory = Split-Path -Parent $Path
    if (-not (Test-Path $directory)) {
        New-Item -ItemType Directory -Force -Path $directory | Out-Null
    }

    Set-Content -Path $Path -Value $document -Encoding UTF8
}

function Clamp-Score {
    param([double]$Value)
    return [math]::Max(0, [math]::Min(100, [math]::Round($Value, 2)))
}

function Get-PercentGrade {
    param([double]$Score)

    if ($Score -ge 90) { return "A" }
    elseif ($Score -ge 80) { return "B" }
    elseif ($Score -ge 70) { return "C" }
    elseif ($Score -ge 60) { return "D" }
    elseif ($Score -ge 50) { return "E" }
    return "F"
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

function Format-GradeBadge {
    param([string]$Grade)
    return '<span class="badge {0}">{1}</span>' -f (Get-GradeBadgeClass -Grade $Grade), (Escape-Html $Grade)
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

function Convert-ToRepoPath {
    param([string]$Path)

    $fullPath = [System.IO.Path]::GetFullPath($Path)
    if ($fullPath.StartsWith($repoRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
        return ($fullPath.Substring($repoRoot.Length).TrimStart('\', '/')) -replace '\\', '/'
    }

    return $Path -replace '\\', '/'
}

function Get-Percent {
    param(
        [int]$Numerator,
        [int]$Denominator
    )

    if ($Denominator -le 0) {
        return 100.0
    }

    return ([double]$Numerator / [double]$Denominator) * 100.0
}

function Get-LowerIsBetterScore {
    param(
        [double]$Value,
        [double]$A,
        [double]$B,
        [double]$C,
        [double]$D,
        [double]$E
    )

    if ($Value -le $A) { return 100.0 }
    elseif ($Value -le $B) { return 85.0 }
    elseif ($Value -le $C) { return 70.0 }
    elseif ($Value -le $D) { return 55.0 }
    elseif ($Value -le $E) { return 40.0 }
    return 20.0
}

function Get-OptionalJsonValue {
    param(
        [AllowNull()][object]$InputObject,
        [string[]]$Path,
        $DefaultValue = $null
    )

    $current = $InputObject
    foreach ($segment in $Path) {
        if ($null -eq $current) {
            return $DefaultValue
        }

        $property = $current.PSObject.Properties[$segment]
        if ($null -eq $property) {
            return $DefaultValue
        }

        $current = $property.Value
    }

    return $current
}

function Get-IndentDepth {
    param([string]$Line)

    $depth = 0
    foreach ($char in $Line.ToCharArray()) {
        if ($char -eq "`t") {
            $depth += 4
        }
        elseif ($char -eq ' ') {
            $depth += 1
        }
        else {
            break
        }
    }

    return $depth
}

function New-Finding {
    param(
        [string]$Severity,
        [string]$Category,
        [string]$Path,
        [int]$Line,
        [string]$Message,
        [string]$Guidance
    )

    return [pscustomobject]@{
        Severity = $Severity
        Category = $Category
        Path = $Path
        Line = $Line
        Message = $Message
        Guidance = $Guidance
    }
}

function Get-FindingSeverityRank {
    param([string]$Severity)

    switch ($Severity) {
        "high" { return 3 }
        "medium" { return 2 }
        "low" { return 1 }
        default { return 0 }
    }
}

function Get-DocGuidance {
    return @(
        [pscustomobject]@{ Title = "Static typing in GDScript"; Url = "https://docs.godotengine.org/en/stable/tutorials/scripting/gdscript/static_typing.html"; Guidance = "Typed GDScript improves performance when operand and argument types are known at compile time, and the docs recommend staying consistent within a codebase." },
        [pscustomobject]@{ Title = "CPU optimization"; Url = "https://docs.godotengine.org/en/stable/tutorials/performance/cpu_optimization.html"; Guidance = "Profile bottlenecks first, manually time the hot functions when needed, and remember that _process() / _physics_process() propagation and large node counts have a cost." },
        [pscustomobject]@{ Title = "General optimization tips"; Url = "https://docs.godotengine.org/en/stable/tutorials/performance/general_optimization.html"; Guidance = "Optimize incrementally, prioritize real bottlenecks, and prefer data access patterns with locality instead of scattered work." },
        [pscustomobject]@{ Title = "Array"; Url = "https://docs.godotengine.org/en/stable/classes/class_array.html"; Guidance = "push_front(), pop_front(), and pop_at() shift elements and can have a noticeable performance cost on larger arrays." },
        [pscustomobject]@{ Title = "Thread-safe APIs"; Url = "https://docs.godotengine.org/en/stable/tutorials/performance/thread_safe_apis.html"; Guidance = "Threads help only for the right workloads, the active scene tree is not thread-safe, and render-resource work on other threads can stall." },
        [pscustomobject]@{ Title = "GPU optimization"; Url = "https://docs.godotengine.org/en/stable/tutorials/performance/gpu_optimization.html"; Guidance = "2D performance benefits from minimizing instructions and batching similar work instead of issuing many individual draw operations." }
    )
}

function Get-FunctionInventory {
    param(
        [string]$Path,
        [string[]]$Lines
    )

    $signatures = [System.Collections.Generic.List[object]]::new()
    $signaturePattern = '^\s*(?:static\s+)?func\s+(?<name>[A-Za-z0-9_]+)\s*\((?<params>.*)\)\s*(?<return>->\s*[^:]+)?\s*:'
    for ($index = 0; $index -lt $Lines.Count; $index++) {
        $line = $Lines[$index]
        if ($line -match $signaturePattern) {
            $signatures.Add([pscustomobject]@{
                Name = $matches["name"]
                StartIndex = $index
                StartLine = $index + 1
                ParameterText = $matches["params"]
                HasTypedReturn = -not [string]::IsNullOrWhiteSpace($matches["return"])
                Signature = $line.Trim()
            })
        }
    }

    $functions = [System.Collections.Generic.List[object]]::new()
    for ($index = 0; $index -lt $signatures.Count; $index++) {
        $signature = $signatures[$index]
        $endIndex = if ($index + 1 -lt $signatures.Count) { $signatures[$index + 1].StartIndex - 1 } else { $Lines.Count - 1 }
        $bodyLines = if ($endIndex -gt $signature.StartIndex) { @($Lines[($signature.StartIndex + 1)..$endIndex]) } else { @() }
        $parameters = @()
        if (-not [string]::IsNullOrWhiteSpace($signature.ParameterText)) {
            $parameters = @(
                $signature.ParameterText.Split(',') |
                    ForEach-Object { $_.Trim() } |
                    Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
            )
        }

        $loopLines = @($bodyLines | Select-String -Pattern '^\s*(for|while)\b')
        $queueRedrawLines = @($bodyLines | Select-String -Pattern 'queue_redraw\(')
        $refreshCalls = @($bodyLines | Select-String -Pattern '\b(_refresh_[A-Za-z0-9_]+|_rebuild_[A-Za-z0-9_]+)\(')
        $drawCallLines = @($bodyLines | Select-String -Pattern '\bdraw_[A-Za-z0-9_]+\(')
        $blockingLoadLines = @($bodyLines | Select-String -Pattern '\b(ResourceLoader\.load|load\(|FileAccess\.open|Image\.load|ImageTexture\.create_from_image|HTTPRequest\.request)\(')
        $threadLines = @($bodyLines | Select-String -Pattern '\b(Thread\.new|WorkerThreadPool|call_deferred|set_deferred)\b')
        $allocationLines = @($bodyLines | Select-String -Pattern '(\.new\(|instantiate\(|queue_free\()')

        $loopIndentStack = [System.Collections.Generic.List[int]]::new()
        $nestedLoopCount = 0
        foreach ($bodyLine in $bodyLines) {
            if ($bodyLine -notmatch '^\s*(for|while)\b') {
                continue
            }

            $indentDepth = Get-IndentDepth -Line $bodyLine
            while ($loopIndentStack.Count -gt 0 -and $loopIndentStack[$loopIndentStack.Count - 1] -ge $indentDepth) {
                $loopIndentStack.RemoveAt($loopIndentStack.Count - 1)
            }
            if ($loopIndentStack.Count -gt 0) {
                $nestedLoopCount += 1
            }
            $loopIndentStack.Add($indentDepth)
        }

        $functions.Add([pscustomobject]@{
            Path = $Path
            Name = $signature.Name
            Signature = $signature.Signature
            StartLine = $signature.StartLine
            EndLine = $endIndex + 1
            Lines = ($endIndex - $signature.StartIndex) + 1
            ParameterCount = $parameters.Count
            TypedParameterCount = (@($parameters | Where-Object { $_ -match ':' })).Count
            HasTypedReturn = $signature.HasTypedReturn
            IsFrameFunction = $signature.Name -in @("_process", "_physics_process")
            IsDrawFunction = ($signature.Name -eq "_draw") -or ($signature.Name -like "_draw_*")
            LoopCount = $loopLines.Count
            NestedLoopCount = $nestedLoopCount
            QueueRedrawCount = $queueRedrawLines.Count
            RefreshCallCount = $refreshCalls.Count
            DrawCallCount = $drawCallLines.Count
            BlockingLoadCount = $blockingLoadLines.Count
            ThreadApiCount = $threadLines.Count
            AllocationCount = $allocationLines.Count
            DispatcherPhaseCount = (@($bodyLines | Select-String -Pattern '^\s*_draw_[A-Za-z0-9_]+\(')).Count
            BodyLines = @($bodyLines)
        })
    }

    return @($functions)
}

function Get-FileLineCount {
    param([string[]]$Lines)
    return (@($Lines)).Count
}

function Get-FileSizeGradeScore {
    param([int]$LineCount)

    if ($LineCount -le 250) { return 100.0 }
    elseif ($LineCount -le 500) { return 90.0 }
    elseif ($LineCount -le 800) { return 78.0 }
    elseif ($LineCount -le 1100) { return 64.0 }
    elseif ($LineCount -le 1400) { return 50.0 }
    return 35.0
}

function Get-FunctionLengthGradeScore {
    param([int]$LineCount)

    if ($LineCount -le 20) { return 100.0 }
    elseif ($LineCount -le 40) { return 88.0 }
    elseif ($LineCount -le 60) { return 72.0 }
    elseif ($LineCount -le 80) { return 58.0 }
    elseif ($LineCount -le 120) { return 42.0 }
    return 24.0
}

function Get-FileAnalysis {
    param([string]$Path)

    try {
        $lines = @([System.IO.File]::ReadAllLines($Path))
        $repoPath = Convert-ToRepoPath -Path $Path
        $functions = @(Get-FunctionInventory -Path $repoPath -Lines $lines)

        $explicitTypedVars = (@($lines | Select-String -Pattern '^\s*var\s+[A-Za-z_][A-Za-z0-9_]*\s*:\s*[^=]+(?:=|$)')).Count
        $inferredTypedVars = (@($lines | Select-String -Pattern '^\s*var\s+[A-Za-z_][A-Za-z0-9_]*\s*:=' )).Count
        $dynamicVars = (@($lines | Select-String -Pattern '^\s*var\s+[A-Za-z_][A-Za-z0-9_]*\s*=\s*')).Count
        $frontOps = @($lines | Select-String -Pattern '\b(push_front|pop_front|pop_at)\(')
        $queueFreeLines = @($lines | Select-String -Pattern '\bqueue_free\(')

        $frameFunctions = @($functions | Where-Object { $_.IsFrameFunction })
        $drawFunctions = @($functions | Where-Object { $_.IsDrawFunction })
        $allFunctionLengthScores = @($functions | ForEach-Object { Get-FunctionLengthGradeScore -LineCount $_.Lines })
        $functionLengthScore = if ($allFunctionLengthScores.Count -eq 0) { 100.0 } else { [double](($allFunctionLengthScores | Measure-Object -Average).Average) }
        $fileSizeScore = Get-FileSizeGradeScore -LineCount (Get-FileLineCount -Lines $lines)

        $hotPathAllocationCount = @($frameFunctions + $drawFunctions | ForEach-Object { $_.AllocationCount } | Measure-Object -Sum).Sum
        if ($null -eq $hotPathAllocationCount) { $hotPathAllocationCount = 0 }
        $frameRefreshCalls = @($frameFunctions | ForEach-Object { $_.RefreshCallCount } | Measure-Object -Sum).Sum
        if ($null -eq $frameRefreshCalls) { $frameRefreshCalls = 0 }
        $frameQueueRedraw = @($frameFunctions | ForEach-Object { $_.QueueRedrawCount } | Measure-Object -Sum).Sum
        if ($null -eq $frameQueueRedraw) { $frameQueueRedraw = 0 }
        $frameLoops = @($frameFunctions | ForEach-Object { $_.LoopCount } | Measure-Object -Sum).Sum
        if ($null -eq $frameLoops) { $frameLoops = 0 }
        $drawLoopFunctions = (@($drawFunctions | Where-Object { $_.LoopCount -gt 0 })).Count
        $nestedDrawLoops = @($drawFunctions | ForEach-Object { $_.NestedLoopCount } | Measure-Object -Sum).Sum
        if ($null -eq $nestedDrawLoops) { $nestedDrawLoops = 0 }
        $drawLargeFunctions = (@($drawFunctions | Where-Object { $_.Lines -gt 60 })).Count
        $blockingLoadsInHotPath = @($frameFunctions + $drawFunctions | ForEach-Object { $_.BlockingLoadCount } | Measure-Object -Sum).Sum
        if ($null -eq $blockingLoadsInHotPath) { $blockingLoadsInHotPath = 0 }

        $score = 100.0
        $score -= $dynamicVars * 8.0
        $score -= $frameQueueRedraw * 25.0
        $score -= $frameRefreshCalls * 18.0
        $score -= $frameLoops * 10.0
        $score -= $drawLoopFunctions * 4.0
        $score -= $nestedDrawLoops * 8.0
        $score -= $drawLargeFunctions * 5.0
        $score -= @($frontOps).Count * 15.0
        $score -= $hotPathAllocationCount * 10.0
        $score -= $blockingLoadsInHotPath * 20.0
        $score = ($score * 0.55) + ($fileSizeScore * 0.20) + ($functionLengthScore * 0.25)
        $score = Clamp-Score -Value $score

        return [pscustomobject]@{
            Path = $repoPath
            LineCount = Get-FileLineCount -Lines $lines
            Functions = @($functions)
            FunctionCount = $functions.Count
            ExplicitTypedVarCount = $explicitTypedVars
            InferredTypedVarCount = $inferredTypedVars
            DynamicVarCount = $dynamicVars
            FrontOps = @($frontOps | ForEach-Object { $_.LineNumber })
            QueueFreeLines = @($queueFreeLines | ForEach-Object { $_.LineNumber })
            FrameFunctionCount = $frameFunctions.Count
            DrawFunctionCount = $drawFunctions.Count
            HotPathAllocationCount = $hotPathAllocationCount
            FileSizeScore = $fileSizeScore
            FunctionLengthScore = $functionLengthScore
            Score = $score
            Grade = Get-PercentGrade -Score $score
        }
    }
    catch {
        $scriptLine = if ($null -ne $_.InvocationInfo) { $_.InvocationInfo.ScriptLineNumber } else { 0 }
        $scriptText = if ($null -ne $_.InvocationInfo) { $_.InvocationInfo.Line.Trim() } else { "" }
        throw ("frontend file analysis failed for {0}: {1} (line {2}: {3})" -f (Convert-ToRepoPath -Path $Path), $_.Exception.Message, $scriptLine, $scriptText)
    }
}

function Invoke-FrontendQualityReport {
    $reportPath = Join-Path $OutputRoot "index.html"
    $outputPath = Join-Path $OutputRoot "output.html"
    $summaryPath = Join-Path $OutputRoot "summary.json"

    if (-not (Test-Path $runtimeScriptsRoot)) {
        $body = @"
<h1>Frontend Quality Report Unavailable</h1>
<div class="panel"><p>The runtime Godot scripts folder <code>client/godot/scripts</code> does not exist.</p></div>
<p class="footer"><a href="../index.html">Back to report index</a></p>
"@
        Write-ReportHtml -Path $reportPath -Title "Frontend Quality Report Unavailable" -Body $body
        Write-ReportHtml -Path $outputPath -Title "Frontend Quality Report Unavailable" -Body $body
        return [pscustomobject]@{
            Name = "Frontend Quality"
            Status = "failed"
            Notes = @("Frontend quality analysis could not run because client/godot/scripts was missing.")
            IndexPath = "frontend/index.html"
            ErrorMessage = "client/godot/scripts does not exist."
        }
    }

    try {
        $files = @(Get-ChildItem -Path $runtimeScriptsRoot -Recurse -File -Filter *.gd | Sort-Object FullName)
        if ($files.Count -eq 0) {
            throw "client/godot/scripts does not contain any runtime .gd files."
        }

        $fileAnalyses = @($files | ForEach-Object { Get-FileAnalysis -Path $_.FullName })
        $functions = @($fileAnalyses | ForEach-Object { $_.Functions })
        $docGuidance = Get-DocGuidance

        $totalVarCount = @($fileAnalyses | ForEach-Object { $_.ExplicitTypedVarCount + $_.InferredTypedVarCount + $_.DynamicVarCount } | Measure-Object -Sum).Sum
        $typedVarCount = @($fileAnalyses | ForEach-Object { $_.ExplicitTypedVarCount + $_.InferredTypedVarCount } | Measure-Object -Sum).Sum
        $dynamicVarCount = @($fileAnalyses | ForEach-Object { $_.DynamicVarCount } | Measure-Object -Sum).Sum
        $totalParamCount = @($functions | ForEach-Object { $_.ParameterCount } | Measure-Object -Sum).Sum
        $typedParamCount = @($functions | ForEach-Object { $_.TypedParameterCount } | Measure-Object -Sum).Sum
        $typedReturnCount = (@($functions | Where-Object { $_.HasTypedReturn })).Count
        $frameFunctions = @($functions | Where-Object { $_.IsFrameFunction })
        $drawFunctions = @($functions | Where-Object { $_.IsDrawFunction })
        $frameQueueRedrawCount = @($frameFunctions | ForEach-Object { $_.QueueRedrawCount } | Measure-Object -Sum).Sum
        $frameRefreshCallCount = @($frameFunctions | ForEach-Object { $_.RefreshCallCount } | Measure-Object -Sum).Sum
        $frameLoopCount = @($frameFunctions | ForEach-Object { $_.LoopCount } | Measure-Object -Sum).Sum
        $drawLoopFunctionCount = (@($drawFunctions | Where-Object { $_.LoopCount -gt 0 })).Count
        $nestedDrawLoopCount = @($drawFunctions | ForEach-Object { $_.NestedLoopCount } | Measure-Object -Sum).Sum
        $largeDrawFunctionCount = (@($drawFunctions | Where-Object { $_.Lines -gt 60 })).Count
        $drawDispatcherPhaseCount = @($drawFunctions | Where-Object { $_.Name -eq "_draw" } | ForEach-Object { $_.DispatcherPhaseCount } | Measure-Object -Maximum).Maximum
        $frontOpsCount = @($fileAnalyses | ForEach-Object { @($_.FrontOps).Count } | Measure-Object -Sum).Sum
        $queueFreeCount = @($fileAnalyses | ForEach-Object { @($_.QueueFreeLines).Count } | Measure-Object -Sum).Sum
        $hotPathAllocationCount = @($fileAnalyses | ForEach-Object { $_.HotPathAllocationCount } | Measure-Object -Sum).Sum
        $blockingLoadInHotPathCount = @($functions | Where-Object { $_.IsFrameFunction -or $_.IsDrawFunction } | ForEach-Object { $_.BlockingLoadCount } | Measure-Object -Sum).Sum
        $threadApiCount = @($functions | ForEach-Object { $_.ThreadApiCount } | Measure-Object -Sum).Sum

        foreach ($name in @(
            "frameQueueRedrawCount",
            "frameRefreshCallCount",
            "frameLoopCount",
            "nestedDrawLoopCount",
            "drawDispatcherPhaseCount",
            "frontOpsCount",
            "queueFreeCount",
            "hotPathAllocationCount",
            "blockingLoadInHotPathCount",
            "threadApiCount",
            "dynamicVarCount",
            "typedVarCount",
            "totalVarCount",
            "typedParamCount",
            "totalParamCount"
        )) {
            if ($null -eq (Get-Variable -Name $name -ValueOnly)) {
                Set-Variable -Name $name -Value 0
            }
        }

        $typingScore = Clamp-Score -Value (
            (Get-Percent -Numerator $typedVarCount -Denominator $totalVarCount) * 0.40 +
            (Get-Percent -Numerator $typedParamCount -Denominator $totalParamCount) * 0.35 +
            (Get-Percent -Numerator $typedReturnCount -Denominator $functions.Count) * 0.25 -
            ([math]::Min(20.0, $dynamicVarCount * 8.0))
        )
        $frameScore = Clamp-Score -Value (
            100.0 -
            ($frameFunctions.Count * 8.0) -
            ($frameQueueRedrawCount * 25.0) -
            ($frameRefreshCallCount * 18.0) -
            ($frameLoopCount * 10.0) -
            ((@($frameFunctions | Where-Object { $_.Lines -gt 10 })).Count * 5.0)
        )
        $drawLoopDisciplineScore = Clamp-Score -Value (100.0 - [math]::Min(60.0, ($drawLoopFunctionCount * 10.0) + ($nestedDrawLoopCount * 18.0)))
        $drawStructureScore = Clamp-Score -Value (100.0 - [math]::Min(50.0, ($largeDrawFunctionCount * 8.0) + ([math]::Max(0.0, $drawDispatcherPhaseCount - 6.0) * 3.0)))
        $drawScore = Clamp-Score -Value (($drawLoopDisciplineScore * 0.60) + ($drawStructureScore * 0.40))
        $maintainabilityScore = Clamp-Score -Value (((@($fileAnalyses | ForEach-Object { $_.FileSizeScore } | Measure-Object -Average).Average) * 0.45) + ((@($fileAnalyses | ForEach-Object { $_.FunctionLengthScore } | Measure-Object -Average).Average) * 0.55))
        $collectionScore = Clamp-Score -Value (100.0 - ($frontOpsCount * 20.0) - [math]::Min(15.0, $queueFreeCount * 5.0) - ($hotPathAllocationCount * 12.0))
        $concurrencyScore = Clamp-Score -Value (100.0 - ($blockingLoadInHotPathCount * 40.0) - ($(if ($threadApiCount -gt 0) { 12.0 } else { 0.0 })))

        $runtimeMetrics = $null
        if (Test-Path $runtimeMetricsPath) {
            $runtimeMetrics = Get-Content -Path $runtimeMetricsPath -Raw | ConvertFrom-Json
        }

        $runtimeCategory = $null
        $runtimeSummary = $null
        if ($null -ne $runtimeMetrics) {
            $runtimeUiRefreshAvg = [double](Get-OptionalJsonValue -InputObject $runtimeMetrics -Path @("custom", "ui_refresh_ms", "avg") -DefaultValue 0.0)
            $runtimeArenaDrawAvg = [double](Get-OptionalJsonValue -InputObject $runtimeMetrics -Path @("custom", "arena_draw_ms", "avg") -DefaultValue 0.0)
            $runtimeArenaBaseDrawAvg = [double](Get-OptionalJsonValue -InputObject $runtimeMetrics -Path @("custom", "arena_base_draw_ms", "avg") -DefaultValue 0.0)
            $runtimeVisibilityAvg = [double](Get-OptionalJsonValue -InputObject $runtimeMetrics -Path @("custom", "arena_visibility_ms", "avg") -DefaultValue 0.0)
            $runtimeCacheSyncAvg = [double](Get-OptionalJsonValue -InputObject $runtimeMetrics -Path @("custom", "arena_cache_sync_ms", "avg") -DefaultValue 0.0)
            $runtimeCacheBackgroundAvg = [double](Get-OptionalJsonValue -InputObject $runtimeMetrics -Path @("custom", "arena_cache_background_ms", "avg") -DefaultValue 0.0)
            $runtimeCacheVisibilityAvg = [double](Get-OptionalJsonValue -InputObject $runtimeMetrics -Path @("custom", "arena_cache_visibility_ms", "avg") -DefaultValue 0.0)
            $runtimeProcessAvg = [double](Get-OptionalJsonValue -InputObject $runtimeMetrics -Path @("built_in", "process_time_ms", "avg") -DefaultValue 0.0)
            $runtimePostCleanupOrphans = [double](Get-OptionalJsonValue -InputObject $runtimeMetrics -Path @("post_cleanup_builtin", "orphan_node_count") -DefaultValue 0.0)

            $runtimeUiRefreshScore = Get-LowerIsBetterScore -Value $runtimeUiRefreshAvg -A 2.0 -B 4.0 -C 8.0 -D 12.0 -E 16.0
            $runtimeArenaDrawScore = Get-LowerIsBetterScore -Value $runtimeArenaDrawAvg -A 8.0 -B 12.0 -C 16.0 -D 20.0 -E 28.0
            $runtimeVisibilityScore = Get-LowerIsBetterScore -Value $runtimeVisibilityAvg -A 4.0 -B 8.0 -C 12.0 -D 16.0 -E 22.0
            $runtimeProcessScore = Get-LowerIsBetterScore -Value $runtimeProcessAvg -A 4.0 -B 6.0 -C 10.0 -D 14.0 -E 20.0
            $runtimeOrphanScore = if ($runtimePostCleanupOrphans -le 0.0) {
                100.0
            }
            elseif ($runtimePostCleanupOrphans -le 1.0) {
                70.0
            }
            elseif ($runtimePostCleanupOrphans -le 2.0) {
                40.0
            }
            else {
                0.0
            }

            $runtimeScore = Clamp-Score -Value (
                ($runtimeUiRefreshScore * 0.30) +
                ($runtimeArenaDrawScore * 0.30) +
                ($runtimeVisibilityScore * 0.20) +
                ($runtimeProcessScore * 0.10) +
                ($runtimeOrphanScore * 0.10)
            )

            $runtimeCategory = [pscustomobject]@{
                Name = "Runtime monitor budgets"
                Weight = 20
                Score = $runtimeScore
                Grade = Get-PercentGrade -Score $runtimeScore
                Detail = "Reference match shell averages: ui_refresh ${runtimeUiRefreshAvg}ms, arena_draw ${runtimeArenaDrawAvg}ms, arena_base_draw ${runtimeArenaBaseDrawAvg}ms, arena_visibility ${runtimeVisibilityAvg}ms, arena_cache_sync ${runtimeCacheSyncAvg}ms, arena_cache_background ${runtimeCacheBackgroundAvg}ms, arena_cache_visibility ${runtimeCacheVisibilityAvg}ms, built-in process ${runtimeProcessAvg}ms, post-cleanup orphans ${runtimePostCleanupOrphans}."
            }
            $runtimeSummary = [pscustomobject]@{
                UiRefreshAvgMs = [math]::Round($runtimeUiRefreshAvg, 3)
                ArenaDrawAvgMs = [math]::Round($runtimeArenaDrawAvg, 3)
                ArenaBaseDrawAvgMs = [math]::Round($runtimeArenaBaseDrawAvg, 3)
                ArenaVisibilityAvgMs = [math]::Round($runtimeVisibilityAvg, 3)
                ArenaCacheSyncAvgMs = [math]::Round($runtimeCacheSyncAvg, 3)
                ArenaCacheBackgroundAvgMs = [math]::Round($runtimeCacheBackgroundAvg, 3)
                ArenaCacheVisibilityAvgMs = [math]::Round($runtimeCacheVisibilityAvg, 3)
                ProcessTimeAvgMs = [math]::Round($runtimeProcessAvg, 3)
                PostCleanupOrphanNodeCount = [int][math]::Round($runtimePostCleanupOrphans)
                ArtifactPath = (Convert-ToRepoPath -Path $runtimeMetricsPath)
            }
        }

        $categoryScores = if ($null -ne $runtimeCategory) {
            @(
                [pscustomobject]@{ Name = "Typing discipline"; Weight = 20; Score = $typingScore; Grade = Get-PercentGrade -Score $typingScore; Detail = "Typed vars $typedVarCount / $totalVarCount, typed params $typedParamCount / $totalParamCount, typed returns $typedReturnCount / $($functions.Count)." },
                [pscustomobject]@{ Name = "Frame-loop hygiene"; Weight = 20; Score = $frameScore; Grade = Get-PercentGrade -Score $frameScore; Detail = "$($frameFunctions.Count) frame callbacks, $frameQueueRedrawCount queue_redraw calls, $frameRefreshCallCount refresh/rebuild calls, $frameLoopCount direct loops in frame callbacks." },
                [pscustomobject]@{ Name = "Draw-path efficiency risk"; Weight = 15; Score = $drawScore; Grade = Get-PercentGrade -Score $drawScore; Detail = "$drawLoopFunctionCount draw functions with loops, $nestedDrawLoopCount nested draw loops, $largeDrawFunctionCount large draw helpers, dispatcher fan-out $drawDispatcherPhaseCount." },
                $runtimeCategory,
                [pscustomobject]@{ Name = "Maintainability"; Weight = 10; Score = $maintainabilityScore; Grade = Get-PercentGrade -Score $maintainabilityScore; Detail = "Average runtime file size $([math]::Round((@($fileAnalyses | ForEach-Object { $_.LineCount } | Measure-Object -Average).Average), 1)) lines; $((@($functions | Where-Object { $_.Lines -gt 40 })).Count) functions over 40 lines." },
                [pscustomobject]@{ Name = "Collection and allocation hygiene"; Weight = 10; Score = $collectionScore; Grade = Get-PercentGrade -Score $collectionScore; Detail = "$frontOpsCount front-shifting array ops, $queueFreeCount queue_free calls, $hotPathAllocationCount allocations in frame/draw paths." },
                [pscustomobject]@{ Name = "Blocking I/O and concurrency hygiene"; Weight = 5; Score = $concurrencyScore; Grade = Get-PercentGrade -Score $concurrencyScore; Detail = "$blockingLoadInHotPathCount blocking load calls in hot paths, $threadApiCount thread/deferred APIs touched." }
            )
        }
        else {
            @(
                [pscustomobject]@{ Name = "Typing discipline"; Weight = 25; Score = $typingScore; Grade = Get-PercentGrade -Score $typingScore; Detail = "Typed vars $typedVarCount / $totalVarCount, typed params $typedParamCount / $totalParamCount, typed returns $typedReturnCount / $($functions.Count)." },
                [pscustomobject]@{ Name = "Frame-loop hygiene"; Weight = 25; Score = $frameScore; Grade = Get-PercentGrade -Score $frameScore; Detail = "$($frameFunctions.Count) frame callbacks, $frameQueueRedrawCount queue_redraw calls, $frameRefreshCallCount refresh/rebuild calls, $frameLoopCount direct loops in frame callbacks." },
                [pscustomobject]@{ Name = "Draw-path efficiency risk"; Weight = 20; Score = $drawScore; Grade = Get-PercentGrade -Score $drawScore; Detail = "$drawLoopFunctionCount draw functions with loops, $nestedDrawLoopCount nested draw loops, $largeDrawFunctionCount large draw helpers, dispatcher fan-out $drawDispatcherPhaseCount." },
                [pscustomobject]@{ Name = "Maintainability"; Weight = 10; Score = $maintainabilityScore; Grade = Get-PercentGrade -Score $maintainabilityScore; Detail = "Average runtime file size $([math]::Round((@($fileAnalyses | ForEach-Object { $_.LineCount } | Measure-Object -Average).Average), 1)) lines; $((@($functions | Where-Object { $_.Lines -gt 40 })).Count) functions over 40 lines." },
                [pscustomobject]@{ Name = "Collection and allocation hygiene"; Weight = 10; Score = $collectionScore; Grade = Get-PercentGrade -Score $collectionScore; Detail = "$frontOpsCount front-shifting array ops, $queueFreeCount queue_free calls, $hotPathAllocationCount allocations in frame/draw paths." },
                [pscustomobject]@{ Name = "Blocking I/O and concurrency hygiene"; Weight = 10; Score = $concurrencyScore; Grade = Get-PercentGrade -Score $concurrencyScore; Detail = "$blockingLoadInHotPathCount blocking load calls in hot paths, $threadApiCount thread/deferred APIs touched." }
            )
        }

        $overallScore = 0.0
        foreach ($category in $categoryScores) {
            $overallScore += $category.Score * ($category.Weight / 100.0)
        }

        $findings = [System.Collections.Generic.List[object]]::new()
        foreach ($fileAnalysis in $fileAnalyses) {
            if ($fileAnalysis.DynamicVarCount -gt 0) {
                $dynamicVarLine = @(Get-Content -Path (Join-Path $repoRoot $fileAnalysis.Path) | Select-String -Pattern '^\s*var\s+[A-Za-z_][A-Za-z0-9_]*\s*=\s*' | Select-Object -First 1 -ExpandProperty LineNumber)
                $lineNumber = if ((@($dynamicVarLine)).Count -gt 0) { [int]$dynamicVarLine[0] } else { 1 }
                $findings.Add((New-Finding -Severity "low" -Category "Typing discipline" -Path $fileAnalysis.Path -Line $lineNumber -Message "This script still uses dynamic var assignment instead of explicit typing or := inference." -Guidance "Godot 4.6 docs recommend a consistent typed style and note typed GDScript uses optimized opcodes when types are known at compile time."))
            }
            if ($fileAnalysis.LineCount -gt 1200) {
                $findings.Add((New-Finding -Severity "medium" -Category "Maintainability" -Path $fileAnalysis.Path -Line 1 -Message ("This runtime script is {0} lines long, which makes the file harder to reason about and review." -f $fileAnalysis.LineCount) -Guidance "Large scripts are not automatically slow, but they correlate strongly with long functions, mixed responsibilities, and harder-to-isolate frontend regressions."))
            }
            foreach ($function in @($fileAnalysis.Functions | Where-Object { $_.Lines -gt 80 })) {
                $findings.Add((New-Finding -Severity "high" -Category "Maintainability" -Path $fileAnalysis.Path -Line $function.StartLine -Message ("{0} spans {1} lines." -f $function.Name, $function.Lines) -Guidance "Long GDScript functions are difficult to profile and tend to hide multiple responsibilities. Split state updates, UI shaping, and hot-path work into smaller helpers."))
            }
            foreach ($function in @($fileAnalysis.Functions | Where-Object { $_.IsFrameFunction -and $_.QueueRedrawCount -gt 0 })) {
                $findings.Add((New-Finding -Severity "high" -Category "Frame-loop hygiene" -Path $fileAnalysis.Path -Line $function.StartLine -Message ("{0} queues redraw work every frame." -f $function.Name) -Guidance "The Godot CPU docs call out _process/_physics_process propagation cost, and the GPU docs recommend minimizing per-frame draw work instead of redrawing everything unconditionally."))
            }
            foreach ($function in @($fileAnalysis.Functions | Where-Object { $_.IsFrameFunction -and $_.RefreshCallCount -gt 0 })) {
                $findings.Add((New-Finding -Severity "high" -Category "Frame-loop hygiene" -Path $fileAnalysis.Path -Line $function.StartLine -Message ("{0} triggers refresh/rebuild work from a frame callback." -f $function.Name) -Guidance "Profile-driven UI updates are safer than rebuilding or reformatting UI every frame."))
            }
            foreach ($function in @($fileAnalysis.Functions | Where-Object { $_.IsDrawFunction -and $_.NestedLoopCount -gt 0 })) {
                $findings.Add((New-Finding -Severity "high" -Category "Draw-path efficiency risk" -Path $fileAnalysis.Path -Line $function.StartLine -Message ("{0} performs nested loops inside the custom draw path." -f $function.Name) -Guidance "The Godot GPU docs emphasize 2D batching and reducing draw work. Nested per-tile loops in draw code are high-risk when they execute every frame."))
            }
            foreach ($function in @($fileAnalysis.Functions | Where-Object { $_.IsDrawFunction -and $_.Lines -gt 60 })) {
                $findings.Add((New-Finding -Severity "medium" -Category "Draw-path efficiency risk" -Path $fileAnalysis.Path -Line $function.StartLine -Message ("{0} is a large draw helper at {1} lines." -f $function.Name, $function.Lines) -Guidance "Large draw helpers make it harder to isolate which part of the render path is actually expensive. Prefer smaller, measurable draw passes."))
            }
            foreach ($lineNumber in @($fileAnalysis.FrontOps)) {
                $findings.Add((New-Finding -Severity "medium" -Category "Collection and allocation hygiene" -Path $fileAnalysis.Path -Line $lineNumber -Message "This script uses a front-shifting Array operation." -Guidance "Godot's Array docs note that push_front(), pop_front(), and pop_at() shift the remaining elements and can have a noticeable performance cost on larger arrays."))
            }
        }

        $topFindings = @($findings | Sort-Object @{ Expression = { -1 * (Get-FindingSeverityRank -Severity $_.Severity) } }, @{ Expression = { $_.Path } }, @{ Expression = { $_.Line } } | Select-Object -First 20)
        $formulaText = if ($null -ne $runtimeCategory) {
            "20% typing + 20% frame-loop hygiene + 15% draw-path efficiency + 20% runtime monitor budgets + 10% maintainability + 10% collection/allocation hygiene + 5% blocking-I/O/concurrency hygiene"
        }
        else {
            "25% typing + 25% frame-loop hygiene + 20% draw-path efficiency + 10% maintainability + 10% collection/allocation hygiene + 10% blocking-I/O/concurrency hygiene"
        }
        $scoreBreakdown = @(
            "Typing: $(Format-Score -Score $typingScore)",
            "Frame loops: $(Format-Score -Score $frameScore)",
            "Draw path: $(Format-Score -Score $drawScore)",
            "Maintainability: $(Format-Score -Score $maintainabilityScore)",
            "Collections/allocation: $(Format-Score -Score $collectionScore)",
            "Blocking I/O/concurrency: $(Format-Score -Score $concurrencyScore)"
        )
        if ($null -ne $runtimeCategory) {
            $scoreBreakdown = @("Runtime monitors: $(Format-Score -Score $runtimeCategory.Score)") + $scoreBreakdown
        }
        $overallScoreSummary = New-ScoreSummary -Score $overallScore -Formula $formulaText -Breakdown $scoreBreakdown

        $summaryObject = [pscustomobject]@{
            GeneratedAtUtc = (Get-Date).ToUniversalTime().ToString("o")
            GodotDocsVersion = $godotDocsVersion
            RuntimeScriptRoot = (Convert-ToRepoPath -Path $runtimeScriptsRoot)
            RuntimeScriptCount = $fileAnalyses.Count
            RuntimeFunctionCount = $functions.Count
            RuntimeMonitorArtifact = if ($null -ne $runtimeSummary) { $runtimeSummary.ArtifactPath } else { $null }
            ScoreSummary = $overallScoreSummary
            Categories = @($categoryScores)
            Metrics = [pscustomobject]@{
                TotalVars = $totalVarCount
                TypedVars = $typedVarCount
                DynamicVars = $dynamicVarCount
                TypedParams = $typedParamCount
                TotalParams = $totalParamCount
                TypedReturns = $typedReturnCount
                TotalFunctions = $functions.Count
                FrameFunctions = $frameFunctions.Count
                DrawFunctions = $drawFunctions.Count
                DrawLoopFunctions = $drawLoopFunctionCount
                NestedDrawLoops = $nestedDrawLoopCount
                LargeDrawFunctions = $largeDrawFunctionCount
                DispatcherPhaseCount = $drawDispatcherPhaseCount
                FrontArrayOps = $frontOpsCount
                QueueFreeCalls = $queueFreeCount
                HotPathAllocations = $hotPathAllocationCount
                BlockingLoadsInHotPath = $blockingLoadInHotPathCount
                ThreadApiCount = $threadApiCount
            }
            RuntimeMetrics = $runtimeSummary
            Files = @($fileAnalyses | Sort-Object Score, Path | ForEach-Object { [pscustomobject]@{ Path = $_.Path; LineCount = $_.LineCount; FunctionCount = $_.FunctionCount; Score = $_.Score; Grade = $_.Grade; DynamicVars = $_.DynamicVarCount; FrameFunctions = $_.FrameFunctionCount; DrawFunctions = $_.DrawFunctionCount; HotPathAllocations = $_.HotPathAllocationCount } })
            Findings = @($topFindings)
            Guidance = @($docGuidance)
        }

        New-Item -ItemType Directory -Force -Path $OutputRoot | Out-Null
        $summaryObject | ConvertTo-Json -Depth 8 | Set-Content -Path $summaryPath -Encoding UTF8
        New-Item -ItemType File -Force -Path (Join-Path $OutputRoot ".nojekyll") | Out-Null

        $categoryRows = foreach ($category in $categoryScores) {
            @"
<tr><td>$(Escape-Html $category.Name)</td><td>$($category.Weight)%</td><td>$(Format-Score -Score $category.Score) $(Format-GradeBadge -Grade $category.Grade)</td><td>$(Escape-Html $category.Detail)</td></tr>
"@
        }
        $fileRows = foreach ($file in ($summaryObject.Files | Select-Object -First 12)) {
            @"
<tr><td><code>$(Escape-Html $file.Path)</code></td><td>$($file.LineCount)</td><td>$($file.FunctionCount)</td><td>$(Format-Score -Score $file.Score) $(Format-GradeBadge -Grade $file.Grade)</td><td>dynamic vars: $($file.DynamicVars), frame funcs: $($file.FrameFunctions), draw funcs: $($file.DrawFunctions), hot-path allocations: $($file.HotPathAllocations)</td></tr>
"@
        }
        $findingRows = foreach ($finding in $topFindings) {
            @"
<tr><td>$(Escape-Html $finding.Severity)</td><td>$(Escape-Html $finding.Category)</td><td><code>$(Escape-Html $finding.Path):$($finding.Line)</code></td><td>$(Escape-Html $finding.Message)</td><td>$(Escape-Html $finding.Guidance)</td></tr>
"@
        }
        $guidanceItems = foreach ($entry in $docGuidance) {
            "<li><a href=`"$(Escape-Html $entry.Url)`">$(Escape-Html $entry.Title)</a>: $(Escape-Html $entry.Guidance)</li>"
        }
        $runtimePanel = if ($null -ne $runtimeSummary) {
@"
<div class="panel"><h2>Runtime monitor reference</h2><table><thead><tr><th>Metric</th><th>Value</th><th>Source</th></tr></thead><tbody>
<tr><td>ui_refresh_ms avg</td><td>$($runtimeSummary.UiRefreshAvgMs)</td><td>Custom monitor <code>Rarena/UIRefreshMs</code></td></tr>
<tr><td>arena_draw_ms avg</td><td>$($runtimeSummary.ArenaDrawAvgMs)</td><td>Custom monitor <code>Rarena/ArenaDrawMs</code></td></tr>
<tr><td>arena_base_draw_ms avg</td><td>$($runtimeSummary.ArenaBaseDrawAvgMs)</td><td>Custom monitor <code>Rarena/ArenaBaseDrawMs</code></td></tr>
<tr><td>arena_visibility_ms avg</td><td>$($runtimeSummary.ArenaVisibilityAvgMs)</td><td>Custom monitor <code>Rarena/ArenaVisibilityMs</code></td></tr>
<tr><td>arena_cache_sync_ms avg</td><td>$($runtimeSummary.ArenaCacheSyncAvgMs)</td><td>Custom monitor <code>Rarena/ArenaCacheSyncMs</code></td></tr>
<tr><td>arena_cache_background_ms avg</td><td>$($runtimeSummary.ArenaCacheBackgroundAvgMs)</td><td>Custom monitor <code>Rarena/ArenaCacheBackgroundMs</code></td></tr>
<tr><td>arena_cache_visibility_ms avg</td><td>$($runtimeSummary.ArenaCacheVisibilityAvgMs)</td><td>Custom monitor <code>Rarena/ArenaCacheVisibilityMs</code></td></tr>
<tr><td>process_time_ms avg</td><td>$($runtimeSummary.ProcessTimeAvgMs)</td><td>Built-in <code>Performance.TIME_PROCESS</code></td></tr>
<tr><td>post-cleanup orphan nodes</td><td>$($runtimeSummary.PostCleanupOrphanNodeCount)</td><td>Built-in <code>Performance.OBJECT_ORPHAN_NODE_COUNT</code></td></tr>
<tr><td>Artifact</td><td colspan="2"><code>$(Escape-Html $runtimeSummary.ArtifactPath)</code></td></tr>
</tbody></table></div>
"@
        }
        else {
@"
<div class="panel"><h2>Runtime monitor reference</h2><p>The runtime monitor artifact is missing. Run <code>./scripts/quality.ps1 frontend</code> or <code>./scripts/quality.ps1 frontend-report</code> with a Godot 4.1+ binary to generate <code>server/target/reports/frontend/runtime_monitors.json</code>.</p></div>
"@
        }

        $body = @"
<h1>Frontend Quality Report</h1>
<p class="muted">Analyzed runtime GDScript under <code>client/godot/scripts</code> against Godot $godotDocsVersion guidance on static typing, CPU and GPU optimization, arrays, and thread safety. The goal is not perfect lint; it is a repeatable grade that exposes maintainability and hot-path risk to both humans and LLMs.</p>
<div class="panel"><div class="grid">
  <div class="metric"><span class="muted">Frontend quality score</span><strong>$(Format-Score -Score $overallScoreSummary.Score) $(Format-GradeBadge -Grade $overallScoreSummary.Grade)</strong><div class="detail">$(Escape-Html $overallScoreSummary.Formula)</div></div>
  <div class="metric"><span class="muted">Runtime scripts</span><strong>$($fileAnalyses.Count)</strong><div class="detail">$($functions.Count) functions</div></div>
  <div class="metric"><span class="muted">Typed vars</span><strong>$typedVarCount / $totalVarCount</strong><div class="detail">dynamic vars: $dynamicVarCount</div></div>
  <div class="metric"><span class="muted">Frame callbacks</span><strong>$($frameFunctions.Count)</strong><div class="detail">$frameQueueRedrawCount redraw calls, $frameRefreshCallCount refresh/rebuild calls</div></div>
  <div class="metric"><span class="muted">Draw-loop risk</span><strong>$drawLoopFunctionCount</strong><div class="detail">$nestedDrawLoopCount nested draw loops across $largeDrawFunctionCount large draw helpers</div></div>
  <div class="metric"><span class="muted">Machine-readable summary</span><strong><a href="./summary.json">summary.json</a></strong><div class="detail">Structured for LLM triage and review.</div></div>
</div></div>
<div class="panel"><h2>Weighted categories</h2><table><thead><tr><th>Category</th><th>Weight</th><th>Score</th><th>What was measured</th></tr></thead><tbody>
$(($categoryRows -join "`n"))
</tbody></table></div>
$runtimePanel
<div class="panel"><h2>Top frontend findings</h2><table><thead><tr><th>Severity</th><th>Category</th><th>Location</th><th>Issue</th><th>Why it matters</th></tr></thead><tbody>
$(($findingRows -join "`n"))
</tbody></table></div>
<div class="panel"><h2>Per-file snapshot</h2><table><thead><tr><th>File</th><th>Lines</th><th>Functions</th><th>Score</th><th>Quick risks</th></tr></thead><tbody>
$(($fileRows -join "`n"))
</tbody></table></div>
<div class="panel"><h2>Docs baseline</h2><ul>
$(($guidanceItems -join "`n"))
</ul></div>
<p class="footer"><a href="../index.html">Back to report index</a></p>
"@

        Write-ReportHtml -Path $reportPath -Title "Frontend Quality Report" -Body $body
        Write-ReportHtml -Path $outputPath -Title "Frontend Quality Report" -Body $body

        return [pscustomobject]@{
            Name = "Frontend Quality"
            Status = "ok"
            Notes = @(
                "Frontend quality is now graded from runtime GDScript under client/godot/scripts, not only from headless smoke checks.",
                "The score is heuristic and docs-backed: static typing, frame callback cost, draw-path risk, arrays, and thread-safety guidance all feed the grade.",
                "The machine-readable frontend summary lives at target/reports/frontend/summary.json for LLM-assisted triage.",
                "Runtime monitor sampling from performance_monitor_checks.gd is included when target/reports/frontend/runtime_monitors.json exists."
            )
            IndexPath = "frontend/index.html"
            ErrorMessage = $null
            ScoreSummary = $overallScoreSummary
            Summary = [pscustomobject]@{
                RuntimeScriptCount = $fileAnalyses.Count
                RuntimeFunctionCount = $functions.Count
                DynamicVarCount = $dynamicVarCount
                FrameFunctionCount = $frameFunctions.Count
                DrawFunctionCount = $drawFunctions.Count
            }
        }
    }
    catch {
        $scriptLine = if ($null -ne $_.InvocationInfo) { $_.InvocationInfo.ScriptLineNumber } else { 0 }
        $scriptText = if ($null -ne $_.InvocationInfo) { $_.InvocationInfo.Line.Trim() } else { "" }
        $errorMessage = if ($scriptLine -gt 0) {
            "{0} (line {1}: {2})" -f $_.Exception.Message, $scriptLine, $scriptText
        }
        else {
            $_.Exception.Message
        }
        $body = @"
<h1>Frontend Quality Report Failed</h1>
<div class="panel"><p>The runtime GDScript quality analysis could not complete.</p><p><code>$(Escape-Html $errorMessage)</code></p></div>
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

Invoke-FrontendQualityReport
