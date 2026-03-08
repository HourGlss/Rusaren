[CmdletBinding()]
param(
    [switch]$IncludeNightly,
    [switch]$IncludeFuzzTools,
    [switch]$VerusOnly,
    [switch]$CallgraphOnly
)

$ErrorActionPreference = "Stop"

$serverRoot = Split-Path -Parent $PSScriptRoot
$toolRoot = Join-Path $serverRoot "tools"
$verusRoot = Join-Path $toolRoot "verus"
$verusCurrentRoot = Join-Path $verusRoot "current"
$verusRelease = "0.2026.03.01.25809cb"
$verusTag = "release/$verusRelease"
$runtime = [System.Runtime.InteropServices.RuntimeInformation]
$isWindows = $runtime::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)
$isLinux = $runtime::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Linux)
$isMacOS = $runtime::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::OSX)

$stableComponents = @("clippy", "rustfmt", "llvm-tools-preview", "rust-analyzer")
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

function Get-VerusAssetName {
    $os = [System.Runtime.InteropServices.RuntimeInformation]::OSDescription
    $architecture = [System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture

    if ($isWindows) {
        return "verus-$verusRelease-x86-win.zip"
    }

    if ($isLinux) {
        return "verus-$verusRelease-x86-linux.zip"
    }

    if ($isMacOS) {
        if ($architecture -eq [System.Runtime.InteropServices.Architecture]::Arm64) {
            return "verus-$verusRelease-arm64-macos.zip"
        }

        return "verus-$verusRelease-x86-macos.zip"
    }

    throw "Unsupported operating system for Verus installation: $os"
}

function Get-VerusExecutableName {
    if ($isWindows) {
        return "verus.exe"
    }

    return "verus"
}

function Install-VerusToolchain {
    $versionJsonPath = Join-Path $verusCurrentRoot "version.json"
    if (-not (Test-Path $versionJsonPath)) {
        return
    }

    $versionInfo = Get-Content $versionJsonPath -Raw | ConvertFrom-Json
    $requiredToolchain = [string]$versionInfo.verus.toolchain
    if (-not [string]::IsNullOrWhiteSpace($requiredToolchain)) {
        rustup toolchain install $requiredToolchain --profile minimal | Out-Host
    }
}

function Install-Verus {
    $verusExecutable = Join-Path $verusCurrentRoot (Get-VerusExecutableName)
    $versionMarker = Join-Path $verusCurrentRoot ".installed-version"

    if ((Test-Path $verusExecutable) -and (Test-Path $versionMarker)) {
        $installedVersion = (Get-Content $versionMarker -Raw).Trim()
        if ($installedVersion -eq $verusRelease) {
            Install-VerusToolchain
            Write-Host "Verus $verusRelease is already installed at $verusCurrentRoot"
            return
        }
    }

    $assetName = Get-VerusAssetName
    $downloadUrl = "https://github.com/verus-lang/verus/releases/download/$verusTag/$assetName"
    $tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("rarena-verus-install-" + [System.Guid]::NewGuid().ToString("N"))
    $archivePath = Join-Path $tempRoot $assetName
    $extractRoot = Join-Path $tempRoot "extract"

    New-Item -ItemType Directory -Force -Path $tempRoot | Out-Null
    New-Item -ItemType Directory -Force -Path $toolRoot | Out-Null

    Invoke-WebRequest -Uri $downloadUrl -OutFile $archivePath
    Expand-Archive -Path $archivePath -DestinationPath $extractRoot

    $bundleRoot = Get-ChildItem -Path $extractRoot -Directory | Select-Object -First 1
    if ($null -eq $bundleRoot) {
        throw "Verus archive did not contain an extracted bundle directory."
    }

    if (Test-Path $verusCurrentRoot) {
        Remove-Item -Recurse -Force -Path $verusCurrentRoot
    }

    New-Item -ItemType Directory -Force -Path $verusCurrentRoot | Out-Null
    Copy-Item -Path (Join-Path $bundleRoot.FullName "*") -Destination $verusCurrentRoot -Recurse -Force
    Set-Content -Path $versionMarker -Value $verusRelease -Encoding ASCII

    if (-not $isWindows) {
        foreach ($binary in @("verus", "cargo-verus", "rust_verify", "z3")) {
            $binaryPath = Join-Path $verusCurrentRoot $binary
            if (Test-Path $binaryPath) {
                & chmod +x $binaryPath
            }
        }
    }

    Install-VerusToolchain

    Write-Host "Installed Verus $verusRelease to $verusCurrentRoot"
}

if ($VerusOnly) {
    Install-Verus
    return
}

if ($CallgraphOnly) {
    rustup toolchain install stable --profile minimal | Out-Host
    foreach ($component in $stableComponents) {
        rustup component add $component --toolchain stable | Out-Host
    }
    return
}

rustup toolchain install stable --profile minimal | Out-Host

foreach ($component in $stableComponents) {
    rustup component add $component --toolchain stable | Out-Host
}

foreach ($tool in $stableTools) {
    rustup run stable cargo install --locked $tool | Out-Host
}

Install-Verus

if ($IncludeNightly -or $IncludeFuzzTools) {
    rustup toolchain install nightly --profile minimal | Out-Host
    rustup component add miri --toolchain nightly | Out-Host

    if ($IncludeNightly) {
        rustup run nightly cargo install --locked cargo-udeps | Out-Host
    }
}

if ($IncludeFuzzTools) {
    rustup run stable cargo install --locked cargo-fuzz | Out-Host
}
