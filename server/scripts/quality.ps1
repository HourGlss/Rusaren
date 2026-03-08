[CmdletBinding()]
param(
    [ValidateSet("all", "fmt", "lint", "hack", "test", "doc", "coverage", "reports", "deny", "audit", "udeps", "miri", "complexity", "callgraph", "bench", "fuzz", "typos", "taplo", "zizmor", "verus")]
    [string]$Task = "all"
)

$ErrorActionPreference = "Stop"

$serverRoot = Split-Path -Parent $PSScriptRoot
$repoRoot = Split-Path -Parent $serverRoot
Set-Location $serverRoot

$cargoBin = Join-Path $HOME ".cargo\\bin"
if (Test-Path $cargoBin) {
    $env:PATH = "$cargoBin$([System.IO.Path]::PathSeparator)$env:PATH"
}

function Invoke-QualityTask {
    param([string]$Name)

    $hasNextest = $null -ne (Get-Command cargo-nextest -ErrorAction SilentlyContinue)

    switch ($Name) {
        "fmt" { rustup run stable cargo xfmt }
        "lint" { rustup run stable cargo xlint }
        "hack" { rustup run stable cargo hack check --workspace --all-targets --each-feature --no-dev-deps }
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
        "coverage" { & (Join-Path $PSScriptRoot "generate-reports.ps1") -Report coverage -FailOnCommandFailure }
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
            if (-not (Test-Path "fuzz/Cargo.toml")) {
                Write-Host "No fuzz workspace has been initialized yet."
                return
            }

            $targets = Get-ChildItem -Path "fuzz/fuzz_targets" -Filter *.rs -File |
                ForEach-Object { [System.IO.Path]::GetFileNameWithoutExtension($_.Name) }

            if ($targets.Count -eq 0) {
                Write-Host "The fuzz workspace exists, but no fuzz targets are defined yet."
                return
            }

            foreach ($target in $targets) {
                rustup run nightly cargo fuzz build $target
            }
        }
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
    @("fmt", "lint", "verus", "hack", "test", "doc", "reports", "deny", "audit", "typos", "taplo", "zizmor")
}
else {
    @($Task)
}

foreach ($name in $tasks) {
    Write-Host "==> $name"
    Invoke-QualityTask -Name $name
}
