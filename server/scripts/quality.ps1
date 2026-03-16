[CmdletBinding()]
param(
    [ValidateSet("all", "fmt", "lint", "hack", "test", "soak", "doc", "docs-artifacts", "coverage", "coverage-gate", "fuzz-coverage", "reports", "deny", "audit", "udeps", "miri", "complexity", "callgraph", "bench", "fuzz", "fuzz-build", "fuzz-live", "mutants", "typos", "taplo", "zizmor", "verus")]
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
        "server_control_event_decode",
        "arena_full_snapshot_decode",
        "arena_delta_snapshot_decode",
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

    $sourceDir = Join-Path $serverRoot ("fuzz\corpus\" + $Target)
    $targetDir = Join-Path $DestinationRoot $Target

    if (Test-Path $targetDir) {
        Remove-Item -Recurse -Force -Path $targetDir
    }
    New-Item -ItemType Directory -Force -Path $targetDir | Out-Null

    if (Test-Path $sourceDir) {
        Copy-Item -Path (Join-Path $sourceDir "*") -Destination $targetDir -Recurse -Force
    }
}

function Invoke-LiveFuzz {
    param([string[]]$Targets)

    if ($isWindowsHost) {
        throw "Live cargo-fuzz execution is not supported on this native Windows/MSVC setup. Use Linux CI, Docker, or WSL for cargo fuzz run."
    }

    if ($Targets.Count -eq 0) {
        Write-Host "The fuzz workspace exists, but no fuzz targets are defined yet."
        return
    }

    $maxTotalTime = 10
    if (-not [string]::IsNullOrWhiteSpace($env:RARENA_FUZZ_MAX_TOTAL_TIME)) {
        $maxTotalTime = [int]$env:RARENA_FUZZ_MAX_TOTAL_TIME
    }
    if ($maxTotalTime -le 0) {
        throw "RARENA_FUZZ_MAX_TOTAL_TIME must be greater than zero."
    }

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

function Invoke-SoakTests {
    param([bool]$HasNextest)

    if ($HasNextest) {
        rustup run stable cargo nextest run -p game_api --test soak_match_flow --all-features
        return
    }

    Write-Host "cargo-nextest is not installed; falling back to cargo test for soak coverage."
    rustup run stable cargo test -p game_api --test soak_match_flow --all-features
}

function Invoke-MutationTesting {
    param([bool]$HasNextest)

    $outputRoot = Join-Path $serverRoot "target\reports\mutants"
    New-Item -ItemType Directory -Force -Path $outputRoot | Out-Null
    $logPath = Join-Path $outputRoot "mutants.log"
    $status = "passed"
    $errorMessage = $null

    $args = @(
        "mutants",
        "--workspace",
        "--output", $outputRoot,
        "--copy-target", "false",
        "--jobs", $(if ([string]::IsNullOrWhiteSpace($env:RARENA_MUTANTS_JOBS)) { "2" } else { $env:RARENA_MUTANTS_JOBS }),
        "--timeout", $(if ([string]::IsNullOrWhiteSpace($env:RARENA_MUTANTS_TIMEOUT)) { "240" } else { $env:RARENA_MUTANTS_TIMEOUT }),
        "--build-timeout", $(if ([string]::IsNullOrWhiteSpace($env:RARENA_MUTANTS_BUILD_TIMEOUT)) { "180" } else { $env:RARENA_MUTANTS_BUILD_TIMEOUT })
    )

    if ($HasNextest) {
        $args += @("--test-tool", "nextest")
    }
    if (-not [string]::IsNullOrWhiteSpace($env:RARENA_MUTANTS_FILE)) {
        $args += @("--file", $env:RARENA_MUTANTS_FILE)
    }
    if (-not [string]::IsNullOrWhiteSpace($env:RARENA_MUTANTS_SHARD)) {
        $args += @("--shard", $env:RARENA_MUTANTS_SHARD)
    }

    try {
        $previousIncrementalValue = $env:CARGO_INCREMENTAL
        try {
            $env:CARGO_INCREMENTAL = "0"
            & rustup run stable cargo @args 2>&1 | Tee-Object -FilePath $logPath
        }
        finally {
            if ($null -eq $previousIncrementalValue) {
                Remove-Item Env:CARGO_INCREMENTAL -ErrorAction SilentlyContinue
            }
            else {
                $env:CARGO_INCREMENTAL = $previousIncrementalValue
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
            if ($isWindowsHost) {
                Write-Host "Live cargo-fuzz execution is not available on native Windows/MSVC in this repo; building ingress fuzz targets instead."
                Invoke-FuzzBuild -Targets $targets
            }
            else {
                Invoke-LiveFuzz -Targets $targets
            }
        }
        "fuzz-build" { Invoke-FuzzBuild -Targets (Get-AllFuzzTargets) }
        "fuzz-live" { Invoke-LiveFuzz -Targets (Get-NetworkFuzzTargets) }
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
