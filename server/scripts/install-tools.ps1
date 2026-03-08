[CmdletBinding()]
param(
    [switch]$IncludeNightly,
    [switch]$IncludeFuzzTools
)

$ErrorActionPreference = "Stop"

$stableComponents = @("clippy", "rustfmt", "llvm-tools-preview")
$stableTools = @(
    "cargo-nextest",
    "cargo-llvm-cov",
    "cargo-deny",
    "cargo-audit",
    "cargo-hack",
    "cargo-mutants",
    "cargo-geiger",
    "rust-code-analysis-cli",
    "taplo-cli",
    "typos-cli",
    "zizmor"
)

rustup toolchain install stable --profile minimal | Out-Host

foreach ($component in $stableComponents) {
    rustup component add $component --toolchain stable | Out-Host
}

foreach ($tool in $stableTools) {
    rustup run stable cargo install --locked $tool | Out-Host
}

if ($IncludeNightly) {
    rustup toolchain install nightly --profile minimal | Out-Host
    rustup component add miri --toolchain nightly | Out-Host
    rustup run nightly cargo install --locked cargo-udeps | Out-Host
}

if ($IncludeFuzzTools) {
    rustup run stable cargo install --locked cargo-fuzz | Out-Host
}
