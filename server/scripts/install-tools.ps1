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
$scipCallgraphRepo = "https://github.com/Beneficial-AI-Foundation/scip-callgraph.git"
$scipCallgraphCommit = "060ae43b054bfb255bc0867cbf2740e20d530725"
$scipCallgraphRoot = Join-Path $toolRoot "scip-callgraph"
$scipCallgraphCurrentRoot = Join-Path $scipCallgraphRoot "current"
$scipCallgraphSourceRoot = Join-Path $scipCallgraphCurrentRoot "src"
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

function Get-ExecutableSuffix {
    if ($isWindows) {
        return ".exe"
    }

    return ""
}

function Install-ScipCallgraph {
    $suffix = Get-ExecutableSuffix
    $versionMarker = Join-Path $scipCallgraphCurrentRoot ".installed-commit"
    $requiredBins = @(
        (Join-Path $scipCallgraphSourceRoot "target\release\generate_call_graph_dot$suffix"),
        (Join-Path $scipCallgraphSourceRoot "target\release\generate_files_subgraph_dot$suffix"),
        (Join-Path $scipCallgraphSourceRoot "target\release\write_atoms_to_svg$suffix")
    )

    if ((Test-Path $versionMarker) -and ((Get-Content $versionMarker -Raw).Trim() -eq $scipCallgraphCommit)) {
        $allBinsPresent = $true
        foreach ($requiredBin in $requiredBins) {
            if (-not (Test-Path $requiredBin)) {
                $allBinsPresent = $false
                break
            }
        }

        if ($allBinsPresent) {
            Write-Host "scip-callgraph $scipCallgraphCommit is already installed at $scipCallgraphSourceRoot"
            return
        }
    }

    if (Test-Path $scipCallgraphCurrentRoot) {
        Remove-Item -Recurse -Force -Path $scipCallgraphCurrentRoot
    }

    New-Item -ItemType Directory -Force -Path $scipCallgraphRoot | Out-Null
    git clone $scipCallgraphRepo $scipCallgraphSourceRoot | Out-Host
    git -C $scipCallgraphSourceRoot checkout $scipCallgraphCommit | Out-Host

    rustup run stable cargo build --release `
        --manifest-path (Join-Path $scipCallgraphSourceRoot "Cargo.toml") `
        --target-dir (Join-Path $scipCallgraphSourceRoot "target") `
        -p metrics-cli `
        --bin generate_call_graph_dot `
        --bin generate_files_subgraph_dot `
        --bin write_atoms_to_svg | Out-Host

    Set-Content -Path $versionMarker -Value $scipCallgraphCommit -Encoding ASCII
    Write-Host "Installed scip-callgraph $scipCallgraphCommit to $scipCallgraphSourceRoot"
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
    Install-ScipCallgraph
    return
}

rustup toolchain install stable --profile minimal | Out-Host

foreach ($component in $stableComponents) {
    rustup component add $component --toolchain stable | Out-Host
}

foreach ($tool in $stableTools) {
    rustup run stable cargo install --locked $tool | Out-Host
}

Install-ScipCallgraph
Install-Verus

if ($IncludeNightly) {
    rustup toolchain install nightly --profile minimal | Out-Host
    rustup component add miri --toolchain nightly | Out-Host
    rustup run nightly cargo install --locked cargo-udeps | Out-Host
}

if ($IncludeFuzzTools) {
    rustup run stable cargo install --locked cargo-fuzz | Out-Host
}
