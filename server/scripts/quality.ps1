[CmdletBinding()]
param(
    [ValidateSet("all", "fmt", "lint", "hack", "test", "doc", "coverage", "deny", "audit", "udeps", "miri", "complexity", "bench", "fuzz", "typos", "taplo", "zizmor")]
    [string]$Task = "all"
)

$ErrorActionPreference = "Stop"

$serverRoot = Split-Path -Parent $PSScriptRoot
$repoRoot = Split-Path -Parent $serverRoot
Set-Location $serverRoot

$cargoBin = Join-Path $HOME ".cargo\\bin"
if (Test-Path $cargoBin) {
    $env:PATH = "$cargoBin;$env:PATH"
}

function Invoke-QualityTask {
    param([string]$Name)

    switch ($Name) {
        "fmt" { rustup run stable cargo xfmt }
        "lint" { rustup run stable cargo xlint }
        "hack" { rustup run stable cargo hack check --workspace --all-targets --each-feature --no-dev-deps }
        "test" { rustup run stable cargo xtest }
        "doc" { rustup run stable cargo xdoc }
        "coverage" { rustup run stable cargo xcov }
        "deny" { rustup run stable cargo xdeny }
        "audit" { rustup run stable cargo xaudit }
        "udeps" { rustup run nightly cargo udeps --workspace --all-targets }
        "miri" { rustup run nightly cargo miri test --workspace }
        "complexity" {
            New-Item -ItemType Directory -Force -Path "target/quality" | Out-Null
            rust-code-analysis-cli --metrics --output-format json --output target/quality --paths crates
        }
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

            rustup run nightly cargo fuzz build
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
    @("fmt", "lint", "hack", "test", "doc", "coverage", "deny", "audit", "typos", "taplo", "zizmor")
}
else {
    @($Task)
}

foreach ($name in $tasks) {
    Write-Host "==> $name"
    Invoke-QualityTask -Name $name
}
