[CmdletBinding()]
param(
    [ValidateSet("all", "fmt", "lint", "hack", "test", "frontend", "frontend-report", "soak", "doc", "docs-artifacts", "coverage", "coverage-gate", "fuzz-coverage", "reports", "deny", "audit", "udeps", "miri", "complexity", "clean-code", "callgraph", "bench", "fuzz", "fuzz-build", "fuzz-live", "fuzz-merge", "mutants", "typos", "taplo", "zizmor", "verus")]
    [string]$Task = "all"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
if ($PSVersionTable.PSVersion.Major -ge 7) {
    $PSNativeCommandUseErrorActionPreference = $true
}

$serverRoot = Split-Path -Parent $PSScriptRoot
$repoRoot = Split-Path -Parent $serverRoot
Set-Location $serverRoot

$cargoBin = Join-Path $HOME ".cargo\\bin"
if (Test-Path $cargoBin) {
    $env:PATH = "$cargoBin$([System.IO.Path]::PathSeparator)$env:PATH"
}

$runtime = [System.Runtime.InteropServices.RuntimeInformation]
$isWindowsHost = $runtime::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)
$nightlyToolchain = if ([string]::IsNullOrWhiteSpace($env:RARENA_NIGHTLY_TOOLCHAIN)) {
    "nightly-2026-03-01"
}
else {
    $env:RARENA_NIGHTLY_TOOLCHAIN
}

function Get-AllFuzzTargets {
    if (-not (Test-Path "fuzz/Cargo.toml")) {
        return @()
    }

    return @(
        Get-ChildItem -Path "fuzz/fuzz_targets" -Filter *.rs -File |
            ForEach-Object { [System.IO.Path]::GetFileNameWithoutExtension($_.Name) }
    )
}

function Get-NetworkFuzzTargets {
    return @(
        "packet_header_decode",
        "control_command_decode",
        "control_command_roundtrip",
        "input_frame_decode",
        "input_frame_roundtrip",
        "session_ingress",
        "session_ingress_sequence",
        "server_control_event_decode",
        "server_control_event_roundtrip",
        "arena_full_snapshot_decode",
        "arena_full_snapshot_roundtrip",
        "arena_delta_snapshot_decode",
        "arena_delta_snapshot_roundtrip",
        "webrtc_signal_message_parse",
        "webrtc_signal_message_roundtrip"
    )
}

function Invoke-FuzzBuild {
    param([string[]]$Targets)

    if ($Targets.Count -eq 0) {
        Write-Host "The fuzz workspace exists, but no fuzz targets are defined yet."
        return
    }

    foreach ($target in $Targets) {
        rustup run $nightlyToolchain cargo fuzz build $target
    }
}

function Copy-FuzzSeedCorpus {
    param(
        [string]$Target,
        [string]$DestinationRoot
    )

    Ensure-FuzzSeedCorpus

    $sourceDir = Join-Path $serverRoot ("target\fuzz-seed-corpus\" + $Target)
    $targetDir = Join-Path $DestinationRoot $Target

    Remove-DirectoryIfExists -Path $targetDir
    New-Item -ItemType Directory -Force -Path $targetDir | Out-Null

    if (Test-Path $sourceDir) {
        Copy-Item -Path (Join-Path $sourceDir "*") -Destination $targetDir -Recurse -Force
    }
}

function Remove-DirectoryIfExists {
    param(
        [Parameter(Mandatory)]
        [string]$Path
    )

    function Clear-FileAttributesForRemoval {
        param([string]$TreePath)

        if (-not $isWindowsHost -or -not (Test-Path $TreePath)) {
            return
        }

        try {
            Get-ChildItem -Path $TreePath -Force -Recurse -ErrorAction SilentlyContinue |
                ForEach-Object {
                    try {
                        $_.Attributes = [System.IO.FileAttributes]::Normal
                    }
                    catch {
                    }
                }
            (Get-Item -LiteralPath $TreePath -Force -ErrorAction SilentlyContinue).Attributes = [System.IO.FileAttributes]::Directory
        }
        catch {
        }
    }

    function Try-RemoveDirectoryTree {
        param([string]$TreePath)

        if (-not (Test-Path $TreePath)) {
            return $true
        }

        Clear-FileAttributesForRemoval -TreePath $TreePath

        try {
            Remove-Item -Recurse -Force -LiteralPath $TreePath -ErrorAction Stop
            return $true
        }
        catch {
            $entries = @(
                Get-ChildItem -LiteralPath $TreePath -Force -ErrorAction SilentlyContinue |
                    Sort-Object -Property FullName -Descending
            )
            foreach ($entry in $entries) {
                try {
                    Remove-Item -Recurse -Force -LiteralPath $entry.FullName -ErrorAction SilentlyContinue
                }
                catch {
                }
            }

            try {
                Remove-Item -Recurse -Force -LiteralPath $TreePath -ErrorAction SilentlyContinue
            }
            catch {
            }

            return -not (Test-Path $TreePath)
        }
    }

    for ($attempt = 1; $attempt -le 6; $attempt++) {
        if (-not (Test-Path $Path)) {
            return
        }

        if (Try-RemoveDirectoryTree -TreePath $Path) {
            return
        }

        Start-Sleep -Milliseconds (150 * $attempt)
    }

    if (Test-Path $Path -and -not (Try-RemoveDirectoryTree -TreePath $Path)) {
        throw "Failed to remove directory tree '$Path' after repeated attempts."
    }
}

function Ensure-FuzzSeedCorpus {
    $seedRoot = Join-Path $serverRoot "target\fuzz-seed-corpus"
    if (Test-Path $seedRoot) {
        return
    }

    rustup run stable cargo run -p fuzz_seed_builder --quiet
}

function Get-FuzzMaxTotalTime {
    $maxTotalTime = 10
    if (-not [string]::IsNullOrWhiteSpace($env:RARENA_FUZZ_MAX_TOTAL_TIME)) {
        $maxTotalTime = [int]$env:RARENA_FUZZ_MAX_TOTAL_TIME
    }
    if ($maxTotalTime -le 0) {
        throw "RARENA_FUZZ_MAX_TOTAL_TIME must be greater than zero."
    }

    return $maxTotalTime
}

function Test-WslAvailable {
    if (-not $isWindowsHost) {
        return $false
    }

    $wslCommand = Get-Command wsl.exe -ErrorAction SilentlyContinue
    if ($null -eq $wslCommand) {
        return $false
    }

    try {
        $distros = @(
            & $wslCommand.Source -l -q 2>$null |
                Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
                ForEach-Object { $_.ToString().Replace([string][char]0, '').Trim() } |
                Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
        )
        return $distros.Count -gt 0
    }
    catch {
        return $false
    }
}

function Get-WslDistribution {
    if (-not (Test-WslAvailable)) {
        return $null
    }

    if (-not [string]::IsNullOrWhiteSpace($env:RARENA_WSL_DISTRO)) {
        return $env:RARENA_WSL_DISTRO
    }

    $wslCommand = (Get-Command wsl.exe -ErrorAction SilentlyContinue).Source
    $distros = @(
        & $wslCommand -l -q 2>$null |
            Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
            ForEach-Object { $_.ToString().Replace([string][char]0, '').Trim() } |
            Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
    )
    if ($distros.Count -eq 0) {
        return $null
    }

    return $distros[0]
}

function Convert-WindowsPathToWsl {
    param([string]$Path)

    $resolved = [System.IO.Path]::GetFullPath($Path)
    $normalized = $resolved -replace '\\', '/'
    if ($normalized -match '^([A-Za-z]):/(.*)$') {
        return "/mnt/$($matches[1].ToLower())/$($matches[2])"
    }

    throw "Unable to convert Windows path to WSL path: $Path"
}

function Convert-ToBashDoubleQuotedLiteral {
    param([string]$Value)

    $escaped = $Value.Replace('\', '\\')
    $escaped = $escaped.Replace('"', '\"')
    $escaped = $escaped.Replace('$', '\$')
    $escaped = $escaped.Replace('`', '\`')
    return '"' + $escaped + '"'
}

function Invoke-WslBashCommand {
    param([string]$Script)

    $distribution = Get-WslDistribution
    if ([string]::IsNullOrWhiteSpace($distribution)) {
        throw "WSL is not available. Install a Linux distribution or set RARENA_WSL_DISTRO."
    }

    $normalizedScript = $Script.Replace("`r`n", "`n").Replace("`r", "`n")
    & (Get-Command wsl.exe -ErrorAction SilentlyContinue).Source -d $distribution -- bash -lc $normalizedScript
}

function Invoke-LiveFuzzNative {
    param([string[]]$Targets)

    if ($Targets.Count -eq 0) {
        Write-Host "The fuzz workspace exists, but no fuzz targets are defined yet."
        return
    }

    $maxTotalTime = Get-FuzzMaxTotalTime
    $artifactRoot = Join-Path $serverRoot "fuzz\artifacts"
    $generatedCorpusRoot = Join-Path $serverRoot "target\fuzz-generated-corpus"
    New-Item -ItemType Directory -Force -Path $artifactRoot | Out-Null
    New-Item -ItemType Directory -Force -Path $generatedCorpusRoot | Out-Null

    foreach ($target in $Targets) {
        Copy-FuzzSeedCorpus -Target $target -DestinationRoot $generatedCorpusRoot
        $corpusDir = Join-Path $generatedCorpusRoot $target
        $artifactDir = Join-Path $artifactRoot $target
        New-Item -ItemType Directory -Force -Path $artifactDir | Out-Null

        rustup run $nightlyToolchain cargo fuzz run $target $corpusDir -- "-artifact_prefix=$artifactDir/" "-max_total_time=$maxTotalTime"
    }
}

function Invoke-LiveFuzzViaWsl {
    param([string[]]$Targets)

    if ($Targets.Count -eq 0) {
        Write-Host "The fuzz workspace exists, but no fuzz targets are defined yet."
        return
    }

    $maxTotalTime = Get-FuzzMaxTotalTime
    $artifactRoot = Join-Path $serverRoot "fuzz\artifacts"
    $generatedCorpusRoot = Join-Path $serverRoot "target\fuzz-generated-corpus"
    New-Item -ItemType Directory -Force -Path $artifactRoot | Out-Null
    New-Item -ItemType Directory -Force -Path $generatedCorpusRoot | Out-Null

    $serverRootWsl = Convert-WindowsPathToWsl -Path $serverRoot

    foreach ($target in $Targets) {
        Copy-FuzzSeedCorpus -Target $target -DestinationRoot $generatedCorpusRoot
        $corpusDir = Join-Path $generatedCorpusRoot $target
        $artifactDir = Join-Path $artifactRoot $target
        New-Item -ItemType Directory -Force -Path $artifactDir | Out-Null

        $serverRootQuoted = Convert-ToBashDoubleQuotedLiteral -Value $serverRootWsl
        $toolchainQuoted = Convert-ToBashDoubleQuotedLiteral -Value $nightlyToolchain
        $targetQuoted = Convert-ToBashDoubleQuotedLiteral -Value $target
        $corpusQuoted = Convert-ToBashDoubleQuotedLiteral -Value (Convert-WindowsPathToWsl -Path $corpusDir)
        $artifactPrefixQuoted = Convert-ToBashDoubleQuotedLiteral -Value ("-artifact_prefix={0}/" -f (Convert-WindowsPathToWsl -Path $artifactDir))
        $maxTotalQuoted = Convert-ToBashDoubleQuotedLiteral -Value ("-max_total_time={0}" -f $maxTotalTime)

        $bashCommand = @"
set -euo pipefail
if [ -f "\$HOME/.cargo/env" ]; then . "\$HOME/.cargo/env"; fi
cd $serverRootQuoted
rustup run $toolchainQuoted cargo fuzz run $targetQuoted $corpusQuoted -- $artifactPrefixQuoted $maxTotalQuoted
"@
        Invoke-WslBashCommand -Script $bashCommand
    }
}

function Invoke-LiveFuzz {
    param([string[]]$Targets)

    if ($isWindowsHost) {
        if (Test-WslAvailable) {
            Invoke-LiveFuzzViaWsl -Targets $Targets
            return
        }

        throw "Live cargo-fuzz execution is not supported on this native Windows/MSVC setup without WSL. Install WSL, run in Linux CI/Docker, or use fuzz-build."
    }

    Invoke-LiveFuzzNative -Targets $Targets
}

function Invoke-FuzzMergeNative {
    param([string[]]$Targets)

    $generatedCorpusRoot = Join-Path $serverRoot "target\fuzz-generated-corpus"
    $seedCorpusRoot = Join-Path $serverRoot "target\fuzz-seed-corpus"

    foreach ($target in $Targets) {
        Ensure-FuzzSeedCorpus
        $seedDir = Join-Path $seedCorpusRoot $target
        $generatedDir = Join-Path $generatedCorpusRoot $target
        if (-not (Test-Path $generatedDir)) {
            continue
        }

        $generatedFiles = @(
            Get-ChildItem -Path $generatedDir -File -Recurse -ErrorAction SilentlyContinue
        )
        if ($generatedFiles.Count -eq 0) {
            continue
        }

        if (-not (Test-Path $seedDir)) {
            New-Item -ItemType Directory -Force -Path $seedDir | Out-Null
        }

        rustup run $nightlyToolchain cargo fuzz run $target -- "-merge=1" $seedDir $generatedDir
    }
}

function Invoke-FuzzMergeViaWsl {
    param([string[]]$Targets)

    $generatedCorpusRoot = Join-Path $serverRoot "target\fuzz-generated-corpus"
    $seedCorpusRoot = Join-Path $serverRoot "target\fuzz-seed-corpus"

    $serverRootWsl = Convert-WindowsPathToWsl -Path $serverRoot

    foreach ($target in $Targets) {
        Ensure-FuzzSeedCorpus
        $seedDir = Join-Path $seedCorpusRoot $target
        $generatedDir = Join-Path $generatedCorpusRoot $target
        if (-not (Test-Path $generatedDir)) {
            continue
        }

        $generatedFiles = @(
            Get-ChildItem -Path $generatedDir -File -Recurse -ErrorAction SilentlyContinue
        )
        if ($generatedFiles.Count -eq 0) {
            continue
        }

        if (-not (Test-Path $seedDir)) {
            New-Item -ItemType Directory -Force -Path $seedDir | Out-Null
        }

        $serverRootQuoted = Convert-ToBashDoubleQuotedLiteral -Value $serverRootWsl
        $toolchainQuoted = Convert-ToBashDoubleQuotedLiteral -Value $nightlyToolchain
        $targetQuoted = Convert-ToBashDoubleQuotedLiteral -Value $target
        $seedQuoted = Convert-ToBashDoubleQuotedLiteral -Value (Convert-WindowsPathToWsl -Path $seedDir)
        $generatedQuoted = Convert-ToBashDoubleQuotedLiteral -Value (Convert-WindowsPathToWsl -Path $generatedDir)

        $bashCommand = @"
set -euo pipefail
if [ -f "\$HOME/.cargo/env" ]; then . "\$HOME/.cargo/env"; fi
cd $serverRootQuoted
rustup run $toolchainQuoted cargo fuzz run $targetQuoted -- "-merge=1" $seedQuoted $generatedQuoted
"@
        Invoke-WslBashCommand -Script $bashCommand
    }
}

function Invoke-FuzzMerge {
    param([string[]]$Targets)

    if ($isWindowsHost) {
        if (Test-WslAvailable) {
            Invoke-FuzzMergeViaWsl -Targets $Targets
            return
        }

        throw "Fuzz corpus merge needs Linux cargo-fuzz or WSL on this Windows host."
    }

    Invoke-FuzzMergeNative -Targets $Targets
}

function Invoke-SoakTests {
    param([bool]$HasNextest)

    if ($HasNextest) {
        rustup run stable cargo nextest run -p game_api --test soak_match_flow --test performance_budget_gates --all-features
        return
    }

    Write-Host "cargo-nextest is not installed; falling back to cargo test for soak and performance budget coverage."
    rustup run stable cargo test -p game_api --test soak_match_flow --test performance_budget_gates --all-features
}

function Get-GodotExecutable {
    if (-not [string]::IsNullOrWhiteSpace($env:GODOT_BIN)) {
        $configured = $env:GODOT_BIN
        if (Test-Path $configured) {
            return (Resolve-Path $configured).Path
        }

        $configuredCommand = Get-Command $configured -ErrorAction SilentlyContinue
        if ($null -ne $configuredCommand) {
            return $configuredCommand.Source
        }

        throw "GODOT_BIN was set, but '$configured' was not found."
    }

    if ($isWindowsHost) {
        $portableConsole = Get-ChildItem -Path (Join-Path $repoRoot "Godot") -File -Filter "Godot*_console.exe" -ErrorAction SilentlyContinue |
            Select-Object -First 1 -ExpandProperty FullName
        if (-not [string]::IsNullOrWhiteSpace($portableConsole)) {
            return $portableConsole
        }

        $bundledConsole = Get-ChildItem -Path (Join-Path $serverRoot "tools\godot") -Recurse -File -Filter "Godot*_console.exe" -ErrorAction SilentlyContinue |
            Select-Object -First 1 -ExpandProperty FullName
        if (-not [string]::IsNullOrWhiteSpace($bundledConsole)) {
            return $bundledConsole
        }

        $installedConsole = Get-ChildItem -Path (Join-Path $env:ProgramFiles "Godot") -Recurse -File -Filter "Godot*_console.exe" -ErrorAction SilentlyContinue |
            Select-Object -First 1 -ExpandProperty FullName
        if (-not [string]::IsNullOrWhiteSpace($installedConsole)) {
            return $installedConsole
        }
    }

    foreach ($candidate in @("godot4", "godot-4", "godot")) {
        $command = Get-Command $candidate -ErrorAction SilentlyContinue
        if ($null -ne $command) {
            return $command.Source
        }
    }

    if (-not $isWindowsHost) {
        foreach ($snapCandidate in @("/snap/bin/godot4", "/snap/bin/godot-4", "/snap/bin/godot")) {
            if (Test-Path $snapCandidate) {
                return $snapCandidate
            }
        }
    }

    throw "No Godot executable was found. Install Godot 4, set GODOT_BIN, or run the dedicated godot-web-smoke workflow."
}

function Invoke-FrontendChecks {
    $godotExe = Get-GodotExecutable
    $projectPath = Join-Path $repoRoot "client\godot"
    $versionOutput = (& $godotExe --version 2>$null | Select-Object -First 1)
    if ($versionOutput -match '(\d+)\.(\d+)') {
        $godotMajor = [int]$matches[1]
        $godotMinor = [int]$matches[2]
        if ($godotMajor -lt 4 -or ($godotMajor -eq 4 -and $godotMinor -lt 1)) {
            throw "Frontend checks require Godot 4.1+ because the WebRTC GDExtension bundle is built for 4.1+. Set GODOT_BIN to a compatible editor/runtime."
        }
    }

    & $godotExe --headless --path $projectPath --quit
    & $godotExe --headless --path $projectPath -s res://tests/protocol_checks.gd
    & $godotExe --headless --path $projectPath -s res://tests/web_export_checks.gd
    & $godotExe --headless --path $projectPath -s res://tests/shell_layout_checks.gd
}

function Get-PreferredMutationScratchRoot {
    $preferredRoot = "F:\game_tests"
    if (Test-Path $preferredRoot) {
        return $preferredRoot
    }

    return $null
}

function Get-MutationScratchLabel {
    $parts = @()

    if (-not [string]::IsNullOrWhiteSpace($env:RARENA_MUTANTS_OUTPUT_DIR)) {
        $resolvedOutputDir = $env:RARENA_MUTANTS_OUTPUT_DIR
        if (-not [System.IO.Path]::IsPathRooted($resolvedOutputDir)) {
            $resolvedOutputDir = Join-Path $repoRoot $resolvedOutputDir
        }

        $resolvedOutputDir = [System.IO.Path]::GetFullPath($resolvedOutputDir)
        $parts += Split-Path -Path $resolvedOutputDir -Leaf
        $parts += Split-Path -Path (Split-Path -Path $resolvedOutputDir -Parent) -Leaf
    }

    if (-not [string]::IsNullOrWhiteSpace($env:RARENA_MUTANTS_SHARD)) {
        $parts += $env:RARENA_MUTANTS_SHARD
    }

    if ($parts.Count -eq 0) {
        return "default"
    }

    $label = ($parts -join "-")
    $label = [regex]::Replace($label, '[^A-Za-z0-9._-]+', '-').Trim('-')
    if ([string]::IsNullOrWhiteSpace($label)) {
        return "default"
    }

    return $label
}

function Resolve-MutationOutputRoot {
    $defaultOutputRoot = Join-Path $serverRoot "target\reports\mutants"
    if ([string]::IsNullOrWhiteSpace($env:RARENA_MUTANTS_OUTPUT_DIR)) {
        return $defaultOutputRoot
    }

    $candidate = $env:RARENA_MUTANTS_OUTPUT_DIR
    if (-not [System.IO.Path]::IsPathRooted($candidate)) {
        $candidate = Join-Path $repoRoot $candidate
    }

    return [System.IO.Path]::GetFullPath($candidate)
}

function Invoke-MutationTesting {
    param([bool]$HasNextest)

    $outputRoot = Resolve-MutationOutputRoot
    $logPath = Join-Path $outputRoot "mutants.log"
    $status = "passed"
    $errorMessage = $null
    $historicalEstimate = $null

    $args = @(
        "mutants",
        "--dir", "server",
        "--config", "server/.cargo/mutants.toml",
        "--gitignore", "true",
        "--output", $outputRoot,
        "--jobs", $(if ([string]::IsNullOrWhiteSpace($env:RARENA_MUTANTS_JOBS)) { "2" } else { $env:RARENA_MUTANTS_JOBS }),
        "--timeout", $(if ([string]::IsNullOrWhiteSpace($env:RARENA_MUTANTS_TIMEOUT)) { "240" } else { $env:RARENA_MUTANTS_TIMEOUT }),
        "--build-timeout", $(if ([string]::IsNullOrWhiteSpace($env:RARENA_MUTANTS_BUILD_TIMEOUT)) { "180" } else { $env:RARENA_MUTANTS_BUILD_TIMEOUT })
    )

    if (-not [string]::IsNullOrWhiteSpace($env:RARENA_MUTANTS_PACKAGE)) {
        $args += @("--package", $env:RARENA_MUTANTS_PACKAGE)
    }
    else {
        $args += "--workspace"
    }

    if ($HasNextest) {
        $args += @("--test-tool", "nextest")
    }
    if (-not [string]::IsNullOrWhiteSpace($env:RARENA_MUTANTS_TEST_PACKAGE)) {
        $args += @("--test-package", $env:RARENA_MUTANTS_TEST_PACKAGE)
    }
    if (-not [string]::IsNullOrWhiteSpace($env:RARENA_MUTANTS_FILE)) {
        $args += @("--file", $env:RARENA_MUTANTS_FILE)
    }
    if (-not [string]::IsNullOrWhiteSpace($env:RARENA_MUTANTS_SHARD)) {
        $args += @("--shard", $env:RARENA_MUTANTS_SHARD)
    }

    function Format-MutationDuration {
        param([double]$Seconds)

        if ($Seconds -lt 60) {
            return ("{0:N0}s" -f [Math]::Round($Seconds))
        }

        $duration = [TimeSpan]::FromSeconds([Math]::Max(0, $Seconds))
        if ($duration.TotalHours -ge 1) {
            return ("{0}h {1}m" -f [int]$duration.TotalHours, $duration.Minutes)
        }

        return ("{0}m {1}s" -f $duration.Minutes, $duration.Seconds)
    }

    function Write-MutationStatus {
        param(
            [string]$Path,
            [hashtable]$Status
        )

        $counts = @{
            caught = [int]$Status.counts.CAUGHT
            missed = [int]$Status.counts.MISSED
            timeout = [int]$Status.counts.TIMEOUT
            unviable = [int]$Status.counts.UNVIABLE
        }
        $payload = [ordered]@{
            start_utc                    = $Status.start_utc
            mutation_start_utc           = $Status.mutation_start_utc
            phase                        = $Status.phase
            total_mutants                = $Status.total_mutants
            completed_mutants            = $Status.completed_mutants
            reported_mutant_seconds_sum  = $Status.reported_mutant_seconds_sum
            baseline_seconds             = $Status.baseline_seconds
            estimated_baseline_seconds   = $Status.estimated_baseline_seconds
            estimated_total_seconds      = $Status.estimated_total_seconds
            counts                       = $counts
        }
        $payload | ConvertTo-Json -Depth 4 | Set-Content -Path $Path -Encoding utf8
    }

    function Write-MutationDiagnostic {
        param([string]$Message)

        Write-Host ("[MD] {0}" -f $Message)
    }

    function Get-MutationRelatedProcesses {
        $repoMarker = ($repoRoot -replace '\\', '\\')
        return @(
            Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
                Where-Object {
                    $_.ProcessId -ne $PID -and (
                        ([string]$_.CommandLine -like '*cargo mutants*') -or
                        ([string]$_.CommandLine -like '*quality.ps1*mutants*') -or
                        (
                            $_.Name -match '^(cargo|rustc)\.exe$' -and
                            [string]$_.CommandLine -match $repoMarker
                        )
                    )
                } |
                Select-Object ProcessId, Name, CommandLine
        )
    }

    function Show-MutationPreflightDiagnostics {
        param([string]$Path)

        Write-MutationDiagnostic "preflight: checking for stale mutation artifacts and related processes"

        if (Test-Path $Path) {
            $heartbeatArtifacts = @(
                Get-ChildItem -Path $Path -Force -ErrorAction SilentlyContinue |
                    Where-Object { $_.Name -like 'heartbeat*' }
            )
            if ($heartbeatArtifacts.Count -gt 0) {
                Write-MutationDiagnostic ("found {0} stale heartbeat artifact(s): {1}" -f $heartbeatArtifacts.Count, (($heartbeatArtifacts | Select-Object -ExpandProperty Name) -join ', '))
            }
            else {
                Write-MutationDiagnostic "found no stale heartbeat artifacts in previous mutants output"
            }

            $previousLogPath = Join-Path $Path "mutants.log"
            if (Test-Path $previousLogPath) {
                $tailLines = @(
                    Get-Content -Path $previousLogPath -Tail 3 -ErrorAction SilentlyContinue |
                        Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
                )
                if ($tailLines.Count -gt 0) {
                    Write-MutationDiagnostic ("previous mutants.log tail: {0}" -f ($tailLines -join ' | '))
                }
                else {
                    Write-MutationDiagnostic "previous mutants.log exists but is empty"
                }
            }
            else {
                Write-MutationDiagnostic "no previous mutants.log found"
            }

            $heartbeatPidFiles = @(
                Get-ChildItem -Path $Path -Filter 'heartbeat.*.pid' -File -Force -ErrorAction SilentlyContinue
            )
            foreach ($pidFile in $heartbeatPidFiles) {
                $pidText = (Get-Content -Path $pidFile.FullName -Raw -ErrorAction SilentlyContinue).Trim()
                $heartbeatPid = 0
                if ([int]::TryParse($pidText, [ref]$heartbeatPid)) {
                    $staleHeartbeatProcess = Get-Process -Id $heartbeatPid -ErrorAction SilentlyContinue
                    if ($null -ne $staleHeartbeatProcess) {
                        Write-MutationDiagnostic ("stopping stale heartbeat watcher PID {0} from {1}" -f $heartbeatPid, $pidFile.Name)
                        Stop-Process -Id $heartbeatPid -Force -ErrorAction SilentlyContinue
                    }
                }
            }
        }
        else {
            Write-MutationDiagnostic "no previous mutants output directory found"
        }

        $relatedProcesses = @(Get-MutationRelatedProcesses)
        if ($relatedProcesses.Count -gt 0) {
            Write-MutationDiagnostic ("found {0} mutation-related process(es) already running:" -f $relatedProcesses.Count)
            foreach ($process in $relatedProcesses) {
                $commandLine = [string]$process.CommandLine
                if ($commandLine.Length -gt 180) {
                    $commandLine = $commandLine.Substring(0, 177) + "..."
                }
                Write-MutationDiagnostic ("  PID {0} {1} :: {2}" -f $process.ProcessId, $process.Name, $commandLine)
            }
        }
        else {
            Write-MutationDiagnostic "found no other mutation-related cargo/powershell/rustc processes"
        }
    }

    function Get-HistoricalMutationEstimate {
        param([string]$Path)

        if (-not (Test-Path $Path)) {
            return $null
        }

        $text = Get-Content $Path -Raw -ErrorAction SilentlyContinue
        if ([string]::IsNullOrWhiteSpace($text)) {
            return $null
        }

        $baselineMatches = [regex]::Matches($text, 'Unmutated baseline in\s+([\d.]+)s build \+\s+([\d.]+)s test')
        $baselineSeconds = $null
        if ($baselineMatches.Count -gt 0) {
            $baselineMatch = $baselineMatches[$baselineMatches.Count - 1]
            $baselineSeconds = [double]$baselineMatch.Groups[1].Value + [double]$baselineMatch.Groups[2].Value
        }

        $outcomeMatches = [regex]::Matches($text, '(?m)^\s*(CAUGHT|MISSED|TIMEOUT|UNVIABLE)\b.*?in\s+([\d.]+)s build \+\s+([\d.]+)s test')
        $completed = 0
        $sumSeconds = 0.0
        foreach ($match in $outcomeMatches) {
            $completed += 1
            $sumSeconds += [double]$match.Groups[2].Value + [double]$match.Groups[3].Value
        }

        $averageMutantSeconds = $null
        if ($completed -gt 0) {
            $averageMutantSeconds = $sumSeconds / $completed
        }

        if ($null -eq $baselineSeconds -and $null -eq $averageMutantSeconds) {
            return $null
        }

        return @{
            baseline_seconds       = $baselineSeconds
            average_mutant_seconds = $averageMutantSeconds
        }
    }

    $historicalEstimate = Get-HistoricalMutationEstimate -Path $logPath
    Show-MutationPreflightDiagnostics -Path $outputRoot

    if (Test-Path $outputRoot) {
        Get-ChildItem -Path $outputRoot -Force -ErrorAction SilentlyContinue |
            Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
    }
    New-Item -ItemType Directory -Force -Path $outputRoot | Out-Null

    try {
        $previousIncrementalValue = $env:CARGO_INCREMENTAL
        $previousRepoRootValue = $env:RARENA_REPO_ROOT
        $previousServerRootValue = $env:RARENA_SERVER_ROOT
        $previousTempValue = $env:TEMP
        $previousTmpValue = $env:TMP
        $previousCargoTargetDir = $env:CARGO_TARGET_DIR
        $heartbeatRunId = [Guid]::NewGuid().ToString("N")
        $heartbeatSentinelPath = Join-Path $outputRoot ("heartbeat.{0}.active" -f $heartbeatRunId)
        $heartbeatStatusPath = Join-Path $outputRoot ("heartbeat.{0}.status.json" -f $heartbeatRunId)
        $heartbeatPidPath = Join-Path $outputRoot ("heartbeat.{0}.pid" -f $heartbeatRunId)
        $heartbeatScriptPath = Join-Path $outputRoot ("heartbeat.{0}.ps1" -f $heartbeatRunId)
        $heartbeatProcess = $null
        $statusInfo = @{
            start_utc                   = (Get-Date).ToUniversalTime().ToString("o")
            mutation_start_utc          = $null
            phase                       = "setup"
            total_mutants               = $null
            completed_mutants           = 0
            reported_mutant_seconds_sum = 0.0
            baseline_seconds            = $null
            estimated_baseline_seconds  = $(if ($null -ne $historicalEstimate) { $historicalEstimate.baseline_seconds } else { $null })
            estimated_total_seconds     = $null
            counts                      = @{
                CAUGHT   = 0
                MISSED   = 0
                TIMEOUT  = 0
                UNVIABLE = 0
            }
        }
        try {
            $env:CARGO_INCREMENTAL = "0"
            $env:RARENA_REPO_ROOT = $repoRoot
            $env:RARENA_SERVER_ROOT = $serverRoot
            $scratchRoot = Get-PreferredMutationScratchRoot
            if ($null -ne $scratchRoot) {
                $mutantsScratchRoot = Join-Path $scratchRoot "mutants"
                $scratchLabel = Get-MutationScratchLabel
                $mutantsRunScratchRoot = Join-Path $mutantsScratchRoot $scratchLabel
                $mutantsTempRoot = Join-Path $mutantsRunScratchRoot "tmp"
                $mutantsTargetRoot = Join-Path $mutantsRunScratchRoot "cargo-target"
                if (Test-Path $mutantsTempRoot) {
                    Get-ChildItem -Path $mutantsTempRoot -Force -ErrorAction SilentlyContinue |
                        Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
                }
                if (Test-Path $mutantsTargetRoot) {
                    Get-ChildItem -Path $mutantsTargetRoot -Force -ErrorAction SilentlyContinue |
                        Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
                }
                New-Item -ItemType Directory -Force -Path $mutantsTempRoot | Out-Null
                New-Item -ItemType Directory -Force -Path $mutantsTargetRoot | Out-Null
                $env:TEMP = $mutantsTempRoot
                $env:TMP = $mutantsTempRoot
                $env:CARGO_TARGET_DIR = $mutantsTargetRoot
                Write-Host "Using mutation scratch space under $mutantsRunScratchRoot"
            }
            Write-Host "Mutation testing is running. Progress lines will appear as mutants finish."
            Write-Host "Live counts track completed mutants: CAUGHT / MISSED / TIMEOUT / UNVIABLE."
            Set-Content -Path $heartbeatSentinelPath -Value "running" -Encoding ascii
            Write-MutationStatus -Path $heartbeatStatusPath -Status $statusInfo
            $escapedSentinelPath = $heartbeatSentinelPath.Replace("'", "''")
            $escapedStatusPath = $heartbeatStatusPath.Replace("'", "''")
            $heartbeatScript = @"
function Get-OutcomeLineCount {
    param([string]`$Path)

    if (-not (Test-Path `$Path)) {
        return 0
    }

    return @(
        Get-Content `$Path -ErrorAction SilentlyContinue |
            Where-Object { -not [string]::IsNullOrWhiteSpace(`$_) }
    ).Count
}

function Get-LatestScenarioLog {
    param([string]`$LogDirectory)

    if (-not (Test-Path `$LogDirectory)) {
        return `$null
    }

    return Get-ChildItem `$LogDirectory -File -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
}

function Get-OutcomeProgress {
    param([string]`$MutantsOutDirectory)

    `$result = [ordered]@{
        caught = 0
        missed = 0
        timeout = 0
        unviable = 0
        completed = 0
        total = `$null
        elapsed_seconds_sum = 0.0
    }

    if (-not (Test-Path `$MutantsOutDirectory)) {
        return `$result
    }

    `$result.caught = Get-OutcomeLineCount (Join-Path `$MutantsOutDirectory 'caught.txt')
    `$result.missed = Get-OutcomeLineCount (Join-Path `$MutantsOutDirectory 'missed.txt')
    `$result.timeout = Get-OutcomeLineCount (Join-Path `$MutantsOutDirectory 'timeout.txt')
    `$result.unviable = Get-OutcomeLineCount (Join-Path `$MutantsOutDirectory 'unviable.txt')
    `$result.completed = [int]`$result.caught + [int]`$result.missed + [int]`$result.timeout + [int]`$result.unviable

    `$outcomesPath = Join-Path `$MutantsOutDirectory 'outcomes.json'
    if (Test-Path `$outcomesPath) {
        try {
            `$outcomesDoc = Get-Content `$outcomesPath -Raw -ErrorAction SilentlyContinue | ConvertFrom-Json
            if (`$null -ne `$outcomesDoc) {
                if (`$null -ne `$outcomesDoc.total_mutants) {
                    `$result.total = [int]`$outcomesDoc.total_mutants
                }
                if (`$null -ne `$outcomesDoc.outcomes) {
                    foreach (`$outcome in @(`$outcomesDoc.outcomes)) {
                        foreach (`$phaseResult in @(`$outcome.phase_results)) {
                            if (`$null -ne `$phaseResult.duration) {
                                `$result.elapsed_seconds_sum += [double]`$phaseResult.duration
                            }
                        }
                    }
                }
            }
        }
        catch {
        }
    }

    if (`$null -eq `$result.total) {
        `$mutantsJsonPath = Join-Path `$MutantsOutDirectory 'mutants.json'
        if (Test-Path `$mutantsJsonPath) {
            try {
                `$mutantsDoc = Get-Content `$mutantsJsonPath -Raw -ErrorAction SilentlyContinue | ConvertFrom-Json
                if (`$null -ne `$mutantsDoc) {
                    `$result.total = @(`$mutantsDoc).Count
                }
            }
            catch {
            }
        }
    }

    return `$result
}

`$statusDirectory = Split-Path '$escapedStatusPath' -Parent
`$mutantsOutDirectory = Join-Path `$statusDirectory 'mutants.out'
`$mutantsLogDirectory = Join-Path `$mutantsOutDirectory 'log'
`$lastObservedCompleted = -1
while (Test-Path '$escapedSentinelPath') {
    if (Test-Path '$escapedStatusPath') {
        try {
            `$status = Get-Content '$escapedStatusPath' -Raw | ConvertFrom-Json
            `$startUtc = [DateTime]::Parse(`$status.start_utc).ToUniversalTime()
            `$elapsed = [int]([DateTime]::UtcNow - `$startUtc).TotalSeconds
            `$clock = ('{0:00}:{1:00}:{2:00}' -f [int][Math]::Floor(`$elapsed / 3600), [int][Math]::Floor((`$elapsed % 3600) / 60), [int](`$elapsed % 60))
            `$phase = [string]`$status.phase
            `$total = [int](`$status.total_mutants | ForEach-Object { if (`$_ -eq `$null) { 0 } else { `$_ } })
            `$completed = [int]`$status.completed_mutants
            `$caught = [int]`$status.counts.caught
            `$missed = [int]`$status.counts.missed
            `$timeout = [int]`$status.counts.timeout
            `$unviable = [int]`$status.counts.unviable
            `$estimatedBaselineSeconds = if (`$status.estimated_baseline_seconds -eq `$null) { 0.0 } else { [double]`$status.estimated_baseline_seconds }
            `$estimatedTotalSeconds = if (`$status.estimated_total_seconds -eq `$null) { 0.0 } else { [double]`$status.estimated_total_seconds }
            `$reportedSeconds = 0.0
            `$outcomeProgress = Get-OutcomeProgress -MutantsOutDirectory `$mutantsOutDirectory
            if (`$outcomeProgress.completed -gt 0) {
                `$caught = [int]`$outcomeProgress.caught
                `$missed = [int]`$outcomeProgress.missed
                `$timeout = [int]`$outcomeProgress.timeout
                `$unviable = [int]`$outcomeProgress.unviable
                `$completed = [int]`$outcomeProgress.completed
                `$phase = 'mutating'
                if (`$total -le 0 -and `$null -ne `$outcomeProgress.total -and [int]`$outcomeProgress.total -gt 0) {
                    `$total = [int]`$outcomeProgress.total
                }
                if (`$outcomeProgress.elapsed_seconds_sum -gt 0.0) {
                    `$reportedSeconds = [double]`$outcomeProgress.elapsed_seconds_sum
                }
                else {
                    `$reportedSeconds = 0.0
                }
                if (`$lastObservedCompleted -ne `$completed) {
                    `$lastObservedCompleted = `$completed
                    if (`$total -gt 0 -and `$completed -gt 0 -and `$reportedSeconds -gt 0.0) {
                        `$avgSeconds = `$reportedSeconds / [double]`$completed
                        `$remainingSeconds = [Math]::Max(0.0, (`$total - `$completed) * `$avgSeconds)
                        `$remainingSpan = [TimeSpan]::FromSeconds(`$remainingSeconds)
                        if (`$remainingSpan.TotalHours -ge 1) {
                            `$etaText = ('ETA {0}h {1}m' -f [int]`$remainingSpan.TotalHours, `$remainingSpan.Minutes)
                        }
                        elseif (`$remainingSpan.TotalMinutes -ge 1) {
                            `$etaText = ('ETA {0}m {1}s' -f `$remainingSpan.Minutes, `$remainingSpan.Seconds)
                        }
                        else {
                            `$etaText = ('ETA {0}s' -f [int][Math]::Round(`$remainingSeconds))
                        }
                        `$percent = [Math]::Round((100.0 * `$completed) / `$total, 1)
                        Write-Host ('[MP {0}] {1}/{2} ({3}%%) MISSED={4} TIMEOUT={5} UNVIABLE={6} {7}' -f `$clock, `$completed, `$total, `$percent, `$missed, `$timeout, `$unviable, `$etaText)
                    }
                    else {
                        Write-Host ('[MP {0}] completed={1} MISSED={2} TIMEOUT={3} UNVIABLE={4}' -f `$clock, `$completed, `$missed, `$timeout, `$unviable)
                    }
                }
            }
            `$latestScenarioLog = Get-LatestScenarioLog -LogDirectory `$mutantsLogDirectory
            `$latestScenarioText = ''
            if (`$null -ne `$latestScenarioLog) {
                `$latestScenarioName = [System.IO.Path]::GetFileNameWithoutExtension(`$latestScenarioLog.Name)
                `$ageSeconds = [Math]::Max(0, [int]([DateTime]::Now - `$latestScenarioLog.LastWriteTime).TotalSeconds)
                `$latestScenarioText = ('; latest={0}; log age={1}s' -f `$latestScenarioName, `$ageSeconds)
            }
            if (`$phase -eq 'setup') {
                if (`$total -gt 0) {
                    if (`$estimatedBaselineSeconds -gt 0.0) {
                        `$setupRemaining = [Math]::Max(0.0, `$estimatedBaselineSeconds - `$elapsed)
                        `$setupSpan = [TimeSpan]::FromSeconds(`$setupRemaining)
                        `$roughTotal = ''
                        if (`$estimatedTotalSeconds -gt 0.0) {
                            `$roughTotalSpan = [TimeSpan]::FromSeconds(`$estimatedTotalSeconds)
                            if (`$roughTotalSpan.TotalHours -ge 1) {
                                `$roughTotal = ('; rough full run ~ {0}h {1}m' -f [int]`$roughTotalSpan.TotalHours, `$roughTotalSpan.Minutes)
                            }
                            elseif (`$roughTotalSpan.TotalMinutes -ge 1) {
                                `$roughTotal = ('; rough full run ~ {0}m {1}s' -f `$roughTotalSpan.Minutes, `$roughTotalSpan.Seconds)
                            }
                            else {
                                `$roughTotal = ('; rough full run ~ {0}s' -f [int][Math]::Round(`$estimatedTotalSeconds))
                            }
                        }
                        if (`$setupSpan.TotalMinutes -ge 1) {
                            Write-Host ('[MH {0}] setup: found {1} mutants; baseline ETA ~ {2}m {3}s{4}' -f `$clock, `$total, `$setupSpan.Minutes, `$setupSpan.Seconds, `$roughTotal)
                        }
                        else {
                            Write-Host ('[MH {0}] setup: found {1} mutants; baseline ETA ~ {2}s{3}' -f `$clock, `$total, [int][Math]::Round(`$setupRemaining), `$roughTotal)
                        }
                    }
                    else {
                        Write-Host ('[MH {0}] setup: found {1} mutants; waiting for baseline to finish...' -f `$clock, `$total)
                    }
                }
                else {
                    Write-Host ('[MH {0}] setup: discovering mutants and building baseline...' -f `$clock)
                }
            }
            elseif (`$phase -eq 'warming_up') {
                `$etaText = ''
                if (`$estimatedTotalSeconds -gt 0.0 -and `$estimatedBaselineSeconds -gt 0.0) {
                    `$estimatedFirstMutantSeconds = [Math]::Max(0.0, `$estimatedTotalSeconds - `$estimatedBaselineSeconds)
                    if (`$total -gt 0) {
                        `$estimatedFirstMutantSeconds = `$estimatedFirstMutantSeconds / `$total
                    }
                    if (`$estimatedFirstMutantSeconds -gt 0.0) {
                        `$warmupStartUtc = if ([string]::IsNullOrWhiteSpace([string]`$status.mutation_start_utc)) { `$startUtc } else { [DateTime]::Parse([string]`$status.mutation_start_utc).ToUniversalTime() }
                        `$warmupElapsed = [Math]::Max(0.0, ([DateTime]::UtcNow - `$warmupStartUtc).TotalSeconds)
                        `$remainingWarmup = [Math]::Max(0.0, `$estimatedFirstMutantSeconds - `$warmupElapsed)
                        `$warmupSpan = [TimeSpan]::FromSeconds(`$remainingWarmup)
                        if (`$warmupSpan.TotalMinutes -ge 1) {
                            `$etaText = ('; first result ETA ~ {0}m {1}s' -f `$warmupSpan.Minutes, `$warmupSpan.Seconds)
                        }
                        else {
                            `$etaText = ('; first result ETA ~ {0}s' -f [int][Math]::Round(`$remainingWarmup))
                        }
                    }
                }
                `$warmupElapsedSeconds = if ([string]::IsNullOrWhiteSpace([string]`$status.mutation_start_utc)) { `$elapsed } else { [int]([DateTime]::UtcNow - [DateTime]::Parse([string]`$status.mutation_start_utc).ToUniversalTime()).TotalSeconds }
                Write-Host ('[MH {0}] warming up: baseline is done; cargo-mutants is building/testing the first mutant; elapsed since baseline={1}s{2}' -f `$clock, `$warmupElapsedSeconds, `$etaText)
                if (-not [string]::IsNullOrWhiteSpace(`$latestScenarioText)) {
                    Write-Host ('[MH {0}] warming up detail{1}' -f `$clock, `$latestScenarioText)
                }
            }
            else {
                `$etaText = 'ETA unavailable'
                if (`$total -gt 0 -and `$completed -gt 0) {
                    `$durationSource = if (`$reportedSeconds -gt 0.0) { `$reportedSeconds } else { [double]`$status.reported_mutant_seconds_sum }
                    `$avgSeconds = `$durationSource / [double]`$completed
                    `$remaining = [Math]::Max(0.0, (`$total - `$completed) * `$avgSeconds)
                    `$remainingSpan = [TimeSpan]::FromSeconds(`$remaining)
                    if (`$remainingSpan.TotalHours -ge 1) {
                        `$etaText = ('ETA {0}h {1}m' -f [int]`$remainingSpan.TotalHours, `$remainingSpan.Minutes)
                    }
                    elseif (`$remainingSpan.TotalMinutes -ge 1) {
                        `$etaText = ('ETA {0}m {1}s' -f `$remainingSpan.Minutes, `$remainingSpan.Seconds)
                    }
                    else {
                        `$etaText = ('ETA {0}s' -f [int][Math]::Round(`$remaining))
                    }
                    `$percent = [Math]::Round((100.0 * `$completed) / `$total, 1)
                    Write-Host ('[MH {0}] progress: {1}/{2} ({3}%%) CAUGHT={4} MISSED={5} TIMEOUT={6} UNVIABLE={7} {8}{9}' -f `$clock, `$completed, `$total, `$percent, `$caught, `$missed, `$timeout, `$unviable, `$etaText, `$latestScenarioText)
                }
                else {
                    Write-Host ('[MH {0}] progress: completed={1} CAUGHT={2} MISSED={3} TIMEOUT={4} UNVIABLE={5}; waiting for enough data for ETA...{6}' -f `$clock, `$completed, `$caught, `$missed, `$timeout, `$unviable, `$latestScenarioText)
                }
            }
        }
        catch {
            Write-Host ('[MH {0}] still running...' -f `$clock)
        }
    }
    else {
        Write-Host ('[MH {0}] still running...' -f `$clock)
    }
    Start-Sleep -Seconds 30
}
"@
            Set-Content -Path $heartbeatScriptPath -Value $heartbeatScript -Encoding utf8
            $heartbeatProcess = Start-Process powershell `
                -ArgumentList @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $heartbeatScriptPath) `
                -NoNewWindow `
                -PassThru
            Set-Content -Path $heartbeatPidPath -Value $heartbeatProcess.Id -Encoding ascii
            $mutationOutcomeCounts = @{
                CAUGHT   = 0
                MISSED   = 0
                TIMEOUT  = 0
                UNVIABLE = 0
            }
            Push-Location $repoRoot
            try {
                & rustup run stable cargo @args 2>&1 |
                    Tee-Object -FilePath $logPath |
                    ForEach-Object {
                        $_
                        $line = $_.ToString()
                        if ($line -match 'Found\s+(\d+)\s+mutants to test') {
                            $statusInfo.total_mutants = [int]$matches[1]
                            if ($null -ne $historicalEstimate -and $null -ne $historicalEstimate.average_mutant_seconds) {
                                $estimatedTotal = [double]$statusInfo.total_mutants * [double]$historicalEstimate.average_mutant_seconds
                                if ($null -ne $statusInfo.estimated_baseline_seconds) {
                                    $estimatedTotal += [double]$statusInfo.estimated_baseline_seconds
                                }
                                $statusInfo.estimated_total_seconds = $estimatedTotal
                            }
                            Write-MutationStatus -Path $heartbeatStatusPath -Status $statusInfo
                        }
                        elseif ($line -match 'Unmutated baseline in\s+([\d.]+)s build \+\s+([\d.]+)s test') {
                            $statusInfo.phase = "warming_up"
                            $statusInfo.mutation_start_utc = (Get-Date).ToUniversalTime().ToString("o")
                            $statusInfo.baseline_seconds = [double]$matches[1] + [double]$matches[2]
                            Write-MutationStatus -Path $heartbeatStatusPath -Status $statusInfo
                            $setupElapsed = [int]((Get-Date).ToUniversalTime() - [DateTime]::Parse($statusInfo.start_utc).ToUniversalTime()).TotalSeconds
                            $setupClock = ('{0:00}:{1:00}:{2:00}' -f [int][Math]::Floor($setupElapsed / 3600), [int][Math]::Floor(($setupElapsed % 3600) / 60), [int]($setupElapsed % 60))
                            Write-Host ("[MP {0}] setup finished in {1}; baseline={2}; mutating {3} mutants" -f `
                                $setupClock,
                                (Format-MutationDuration $setupElapsed),
                                (Format-MutationDuration $statusInfo.baseline_seconds),
                                $(if ($null -eq $statusInfo.total_mutants) { "?" } else { $statusInfo.total_mutants }))
                        }
                        if ($line -match '^\s*(CAUGHT|MISSED|TIMEOUT|UNVIABLE)\b') {
                            $outcome = $matches[1]
                            if ($statusInfo.phase -ne "mutating") {
                                $statusInfo.phase = "mutating"
                            }
                            $mutationOutcomeCounts[$outcome] = [int]$mutationOutcomeCounts[$outcome] + 1
                            $statusInfo.counts[$outcome] = $mutationOutcomeCounts[$outcome]
                            $statusInfo.completed_mutants = [int]$mutationOutcomeCounts.CAUGHT + [int]$mutationOutcomeCounts.MISSED + [int]$mutationOutcomeCounts.TIMEOUT + [int]$mutationOutcomeCounts.UNVIABLE
                            if ($line -match 'in\s+([\d.]+)s build \+\s+([\d.]+)s test') {
                                $statusInfo.reported_mutant_seconds_sum = [double]$statusInfo.reported_mutant_seconds_sum + [double]$matches[1] + [double]$matches[2]
                            }
                            Write-MutationStatus -Path $heartbeatStatusPath -Status $statusInfo

                            $progressBits = @(
                                "MISSED=$($mutationOutcomeCounts.MISSED)"
                                "TIMEOUT=$($mutationOutcomeCounts.TIMEOUT)"
                                "UNVIABLE=$($mutationOutcomeCounts.UNVIABLE)"
                            )
                            $runElapsedSeconds = [int]((Get-Date).ToUniversalTime() - [DateTime]::Parse($statusInfo.start_utc).ToUniversalTime()).TotalSeconds
                            $runClock = ('{0:00}:{1:00}:{2:00}' -f [int][Math]::Floor($runElapsedSeconds / 3600), [int][Math]::Floor(($runElapsedSeconds % 3600) / 60), [int]($runElapsedSeconds % 60))
                            if ($statusInfo.total_mutants -and $statusInfo.completed_mutants -gt 0) {
                                $averageSeconds = [double]$statusInfo.reported_mutant_seconds_sum / [double]$statusInfo.completed_mutants
                                $remainingSeconds = [Math]::Max(0.0, ([int]$statusInfo.total_mutants - [int]$statusInfo.completed_mutants) * $averageSeconds)
                                $percent = [Math]::Round((100.0 * [int]$statusInfo.completed_mutants) / [int]$statusInfo.total_mutants, 1)
                                Write-Host ("[MP {0}] {1}/{2} ({3}%) {4} ETA {5}" -f `
                                    $runClock,
                                    $statusInfo.completed_mutants,
                                    $statusInfo.total_mutants,
                                    $percent,
                                    ($progressBits -join " "),
                                    (Format-MutationDuration $remainingSeconds))
                            }
                            else {
                                Write-Host ("[MP {0}] completed={1} {2}" -f `
                                    $runClock,
                                    $statusInfo.completed_mutants,
                                    ($progressBits -join " "))
                            }
                        }
                    }
            }
            finally {
                Pop-Location
            }
        }
        finally {
            Remove-Item -Force $heartbeatSentinelPath -ErrorAction SilentlyContinue
            Remove-Item -Force $heartbeatStatusPath -ErrorAction SilentlyContinue
            Remove-Item -Force $heartbeatPidPath -ErrorAction SilentlyContinue
            Remove-Item -Force $heartbeatScriptPath -ErrorAction SilentlyContinue
            if ($null -ne $heartbeatProcess) {
                try {
                    Wait-Process -Id $heartbeatProcess.Id -Timeout 5 -ErrorAction SilentlyContinue
                }
                catch {
                }
                if (-not $heartbeatProcess.HasExited) {
                    Stop-Process -Id $heartbeatProcess.Id -Force -ErrorAction SilentlyContinue
                }
            }

            if ($null -eq $previousIncrementalValue) {
                Remove-Item Env:CARGO_INCREMENTAL -ErrorAction SilentlyContinue
            }
            else {
                $env:CARGO_INCREMENTAL = $previousIncrementalValue
            }

            if ($null -eq $previousRepoRootValue) {
                Remove-Item Env:RARENA_REPO_ROOT -ErrorAction SilentlyContinue
            }
            else {
                $env:RARENA_REPO_ROOT = $previousRepoRootValue
            }

            if ($null -eq $previousServerRootValue) {
                Remove-Item Env:RARENA_SERVER_ROOT -ErrorAction SilentlyContinue
            }
            else {
                $env:RARENA_SERVER_ROOT = $previousServerRootValue
            }

            if ($null -eq $previousTempValue) {
                Remove-Item Env:TEMP -ErrorAction SilentlyContinue
            }
            else {
                $env:TEMP = $previousTempValue
            }

            if ($null -eq $previousTmpValue) {
                Remove-Item Env:TMP -ErrorAction SilentlyContinue
            }
            else {
                $env:TMP = $previousTmpValue
            }

            if ($null -eq $previousCargoTargetDir) {
                Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue
            }
            else {
                $env:CARGO_TARGET_DIR = $previousCargoTargetDir
            }
        }
    }
    catch {
        $status = "failed"
        $errorMessage = $_.Exception.Message
    }

    $shardLabel = if (-not [string]::IsNullOrWhiteSpace($env:RARENA_MUTANTS_SHARD)) {
        $env:RARENA_MUTANTS_SHARD
    }
    else {
        "1/1"
    }
    $html = @"
<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>Rusaren Mutation Testing</title>
  <style>
    body { font-family: Segoe UI, Arial, sans-serif; margin: 2rem; background: #14181f; color: #f4f6fb; }
    a { color: #7ec8ff; }
    code, pre { background: #1f2630; padding: 0.2rem 0.4rem; border-radius: 0.35rem; }
    .badge { display: inline-block; padding: 0.2rem 0.55rem; border-radius: 999px; background: #20456b; }
    .failed { background: #6b2d2d; }
    .muted { color: #b9c3d4; }
  </style>
</head>
<body>
  <h1>Mutation Testing</h1>
  <p><span class="badge $(if ($status -eq "failed") { "failed" } else { "" })">$status</span></p>
  <p class="muted">Shard <code>$shardLabel</code>. Config: <code>server/.cargo/mutants.toml</code>.</p>
  <p>Primary log: <a href="./mutants.log">mutants.log</a></p>
  $(if ($errorMessage) { "<p><strong>Exit detail:</strong> <code>$errorMessage</code></p>" } else { "" })
</body>
</html>
"@
    Set-Content -Path (Join-Path $outputRoot "index.html") -Value $html -Encoding utf8

    if ($status -eq "failed") {
        throw $errorMessage
    }
}

function Invoke-QualityTask {
    param([string]$Name)

    $hasNextest = $null -ne (Get-Command cargo-nextest -ErrorAction SilentlyContinue)

    switch ($Name) {
        "fmt" { rustup run stable cargo xfmt }
        "lint" { rustup run stable cargo xlint }
        "hack" { rustup run stable cargo hack check --workspace --all-targets --each-feature }
        "test" {
            Ensure-FuzzSeedCorpus
            $nextestJsonPath = $env:RARENA_NEXTEST_JSON_PATH
            if ($hasNextest) {
                $nextestArgs = @("nextest", "run", "--workspace", "--all-features")
                if (-not [string]::IsNullOrWhiteSpace($nextestJsonPath)) {
                    $nextestJsonDirectory = Split-Path -Parent $nextestJsonPath
                    if (-not [string]::IsNullOrWhiteSpace($nextestJsonDirectory)) {
                        New-Item -ItemType Directory -Force -Path $nextestJsonDirectory | Out-Null
                    }
                    $nextestArgs += @("--message-format", "libtest-json-plus", "--status-level", "none", "--final-status-level", "none", "--cargo-quiet")
                    $previousExperimentalValue = $env:NEXTEST_EXPERIMENTAL_LIBTEST_JSON
                    try {
                        $env:NEXTEST_EXPERIMENTAL_LIBTEST_JSON = "1"
                        & rustup run stable cargo @nextestArgs 2>&1 | Tee-Object -FilePath $nextestJsonPath
                    }
                    finally {
                        if ($null -eq $previousExperimentalValue) {
                            Remove-Item Env:NEXTEST_EXPERIMENTAL_LIBTEST_JSON -ErrorAction SilentlyContinue
                        }
                        else {
                            $env:NEXTEST_EXPERIMENTAL_LIBTEST_JSON = $previousExperimentalValue
                        }
                    }
                    break
                }
                rustup run stable cargo @nextestArgs
            }
            else {
                Write-Host "cargo-nextest is not installed; falling back to cargo test."
                if (-not [string]::IsNullOrWhiteSpace($nextestJsonPath)) {
                    Write-Host "Structured nextest export is unavailable without cargo-nextest."
                }
                rustup run stable cargo test --workspace --all-features
            }
        }
        "frontend" { Invoke-FrontendChecks }
        "frontend-report" { & (Join-Path $PSScriptRoot "generate-reports.ps1") -Report frontend -FailOnCommandFailure }
        "soak" { Invoke-SoakTests -HasNextest $hasNextest }
        "doc" { rustup run stable cargo xdoc }
        "docs-artifacts" { & (Join-Path $PSScriptRoot "build-docs.ps1") }
        "coverage" { & (Join-Path $PSScriptRoot "generate-reports.ps1") -Report coverage -FailOnCommandFailure }
        "coverage-gate" { & (Join-Path $PSScriptRoot "check-core-coverage.ps1") }
        "fuzz-coverage" { & (Join-Path $PSScriptRoot "generate-reports.ps1") -Report fuzz -FailOnCommandFailure }
        "reports" { & (Join-Path $PSScriptRoot "generate-reports.ps1") -Report all -FailOnCommandFailure }
        "deny" { rustup run stable cargo xdeny }
        "audit" { rustup run stable cargo xaudit }
        "udeps" { rustup run $nightlyToolchain cargo udeps --workspace --all-targets }
        "miri" { rustup run $nightlyToolchain cargo miri test --workspace }
        "complexity" { & (Join-Path $PSScriptRoot "generate-reports.ps1") -Report complexity -FailOnCommandFailure }
        "clean-code" { & (Join-Path $PSScriptRoot "generate-reports.ps1") -Report clean-code -FailOnCommandFailure }
        "callgraph" { & (Join-Path $PSScriptRoot "generate-reports.ps1") -Report callgraph -FailOnCommandFailure }
        "verus" { & (Join-Path $PSScriptRoot "verus.ps1") }
        "bench" {
            $benchTargets = Get-ChildItem -Path $serverRoot -Recurse -File -Filter *.rs |
                Where-Object { $_.DirectoryName -like "*\\benches" } |
                Select-Object -First 1
            if ($null -eq $benchTargets) {
                Write-Host "No Criterion benchmarks have been added yet."
                return
            }

            rustup run stable cargo bench --workspace --no-run
        }
        "fuzz" {
            $targets = Get-NetworkFuzzTargets
            if ($isWindowsHost -and -not (Test-WslAvailable)) {
                Write-Host "Live cargo-fuzz execution is not available on native Windows/MSVC in this repo without WSL; building ingress fuzz targets instead."
                Invoke-FuzzBuild -Targets $targets
            }
            else {
                Invoke-LiveFuzz -Targets $targets
            }
        }
        "fuzz-build" { Invoke-FuzzBuild -Targets (Get-AllFuzzTargets) }
        "fuzz-live" { Invoke-LiveFuzz -Targets (Get-NetworkFuzzTargets) }
        "fuzz-merge" { Invoke-FuzzMerge -Targets (Get-NetworkFuzzTargets) }
        "mutants" { Invoke-MutationTesting -HasNextest $hasNextest }
        "typos" {
            Push-Location $repoRoot
            try {
                typos .
            }
            finally {
                Pop-Location
            }
        }
        "taplo" {
            Push-Location $repoRoot
            try {
                taplo fmt --check .
            }
            finally {
                Pop-Location
            }
        }
        "zizmor" {
            Push-Location $repoRoot
            try {
                zizmor --offline .
            }
            finally {
                Pop-Location
            }
        }
        default { throw "Unsupported task: $Name" }
    }
}

$tasks = if ($Task -eq "all") {
    @("fmt", "lint", "verus", "hack", "test", "doc", "fuzz", "reports", "coverage-gate", "deny", "audit", "typos", "taplo", "zizmor")
}
else {
    @($Task)
}

foreach ($name in $tasks) {
    Write-Host "==> $name"
    Invoke-QualityTask -Name $name
}
