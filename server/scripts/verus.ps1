[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"

$serverRoot = Split-Path -Parent $PSScriptRoot
$verusRoot = Join-Path $serverRoot "verus"
$repoLocalVerusRoot = Join-Path $serverRoot "tools\verus\current"
$runtime = [System.Runtime.InteropServices.RuntimeInformation]
$isWindowsHost = $runtime::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)
Set-Location $serverRoot

function Get-VerusCommandPath {
    $command = Get-Command verus -ErrorAction SilentlyContinue
    if ($null -ne $command) {
        return $command.Source
    }

    $candidate = Join-Path $repoLocalVerusRoot $(if ($isWindowsHost) { "verus.exe" } else { "verus" })
    if (Test-Path $candidate) {
        return $candidate
    }

    throw "Verus is not installed and no repo-local binary was found at $repoLocalVerusRoot."
}

if (-not (Test-Path $verusRoot)) {
    throw "No Verus models were found at $verusRoot."
}

$models = Get-ChildItem -Path $verusRoot -File -Filter *.rs | Sort-Object Name
if ($models.Count -eq 0) {
    throw "No Verus model files were found at $verusRoot."
}

$verusCommand = Get-VerusCommandPath

foreach ($model in $models) {
    Write-Host "==> verus $($model.Name)"
    & $verusCommand $model.FullName
    if ($LASTEXITCODE -ne 0) {
        throw "Verus verification failed for $($model.Name) with exit code $LASTEXITCODE."
    }
}
