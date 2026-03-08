[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"

$serverRoot = Split-Path -Parent $PSScriptRoot
$verusRoot = Join-Path $serverRoot "verus"
Set-Location $serverRoot

if (-not (Get-Command verus -ErrorAction SilentlyContinue)) {
    throw "Verus is not installed or not on PATH."
}

if (-not (Test-Path $verusRoot)) {
    throw "No Verus models were found at $verusRoot."
}

$models = Get-ChildItem -Path $verusRoot -File -Filter *.rs | Sort-Object Name
if ($models.Count -eq 0) {
    throw "No Verus model files were found at $verusRoot."
}

foreach ($model in $models) {
    Write-Host "==> verus $($model.Name)"
    & verus $model.FullName
    if ($LASTEXITCODE -ne 0) {
        throw "Verus verification failed for $($model.Name) with exit code $LASTEXITCODE."
    }
}
