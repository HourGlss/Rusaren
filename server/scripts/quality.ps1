[CmdletBinding()]
param(
    [ValidateSet("all", "fmt", "lint", "hack", "test", "doc", "docs-artifacts", "coverage", "fuzz-coverage", "reports", "deny", "audit", "udeps", "miri", "complexity", "callgraph", "bench", "fuzz", "fuzz-build", "fuzz-live", "typos", "taplo", "zizmor", "verus")]
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
$isWindows = $runtime::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)

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
        "input_frame_decode",
        "session_ingress",
        "server_control_event_decode",
        "webrtc_signal_message_parse"
    )
}

function Invoke-FuzzBuild {
    param([string[]]$Targets)

    if ($Targets.Count -eq 0) {
        Write-Host "The fuzz workspace exists, but no fuzz targets are defined yet."
        return
    }

    foreach ($target in $Targets) {
        rustup run nightly cargo fuzz build $target
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

    if ($isWindows) {
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

        rustup run nightly cargo fuzz run $target $corpusDir -- "-artifact_prefix=$artifactDir/" "-max_total_time=$maxTotalTime"
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
            if ($hasNextest) {
                rustup run stable cargo nextest run --workspace --all-features
            }
            else {
                Write-Host "cargo-nextest is not installed; falling back to cargo test."
                rustup run stable cargo test --workspace --all-features
            }
        }
        "doc" { rustup run stable cargo xdoc }
        "docs-artifacts" { & (Join-Path $PSScriptRoot "build-docs.ps1") }
        "coverage" { & (Join-Path $PSScriptRoot "generate-reports.ps1") -Report coverage -FailOnCommandFailure }
        "fuzz-coverage" { & (Join-Path $PSScriptRoot "generate-reports.ps1") -Report fuzz -FailOnCommandFailure }
        "reports" { & (Join-Path $PSScriptRoot "generate-reports.ps1") -Report all -FailOnCommandFailure }
        "deny" { rustup run stable cargo xdeny }
        "audit" { rustup run stable cargo xaudit }
        "udeps" { rustup run nightly cargo udeps --workspace --all-targets }
        "miri" { rustup run nightly cargo miri test --workspace }
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
            if ($isWindows) {
                Write-Host "Live cargo-fuzz execution is not available on native Windows/MSVC in this repo; building ingress fuzz targets instead."
                Invoke-FuzzBuild -Targets $targets
            }
            else {
                Invoke-LiveFuzz -Targets $targets
            }
        }
        "fuzz-build" { Invoke-FuzzBuild -Targets (Get-AllFuzzTargets) }
        "fuzz-live" { Invoke-LiveFuzz -Targets (Get-NetworkFuzzTargets) }
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
    @("fmt", "lint", "verus", "hack", "test", "doc", "fuzz", "reports", "deny", "audit", "typos", "taplo", "zizmor")
}
else {
    @($Task)
}

foreach ($name in $tasks) {
    Write-Host "==> $name"
    Invoke-QualityTask -Name $name
}
