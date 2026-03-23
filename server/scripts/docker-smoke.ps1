[CmdletBinding()]
param(
    [int]$Port = 3000,
    [string]$ImageTag = "rusaren/server:local-smoke",
    [switch]$KeepContainer
)

$ErrorActionPreference = "Stop"

$serverRoot = Split-Path -Parent $PSScriptRoot
$repoRoot = Split-Path -Parent $serverRoot
$containerName = "rusaren-server-smoke"
$tempWebRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("rusaren-docker-smoke-" + [guid]::NewGuid().ToString("N"))
$healthUri = "http://127.0.0.1:$Port/healthz"
$metricsUri = "http://127.0.0.1:$Port/metrics"
$rootUri = "http://127.0.0.1:$Port/"

function Invoke-CheckedDockerCommand {
    param(
        [string]$Description,
        [scriptblock]$Command
    )

    Write-Host "==> $Description"
    & $Command
    if ($LASTEXITCODE -ne 0) {
        throw "Docker command failed during '$Description' with exit code $LASTEXITCODE."
    }
}

function Wait-ForHttpBody {
    param(
        [string]$Uri,
        [string]$ExpectedBody,
        [int]$Attempts = 30
    )

    for ($attempt = 1; $attempt -le $Attempts; $attempt++) {
        try {
            $response = Invoke-WebRequest -Uri $Uri -UseBasicParsing -TimeoutSec 2
            if ($response.StatusCode -eq 200 -and $response.Content -eq $ExpectedBody) {
                return
            }
        }
        catch {
        }

        Start-Sleep -Seconds 1
    }

    throw "Timed out waiting for $Uri to return '$ExpectedBody'."
}

function Remove-ContainerIfPresent {
    param([string]$Name)

    try {
        docker rm -f $Name *> $null
    }
    catch {
    }
    finally {
        $global:LASTEXITCODE = 0
    }
}

function New-PlaceholderWebBundle {
    param([string]$Root)

    New-Item -ItemType Directory -Force -Path $Root | Out-Null
    Set-Content -Path (Join-Path $Root "index.html") -Value @"
<!doctype html>
<html>
  <head><meta charset="utf-8"><title>Rusaren Docker Smoke</title></head>
  <body>Rusaren Docker Smoke</body>
</html>
"@ -NoNewline
}

Push-Location $repoRoot
try {
    New-PlaceholderWebBundle -Root $tempWebRoot

    Invoke-CheckedDockerCommand -Description "validate compose file" -Command {
        docker compose --env-file deploy/config.env.example -f deploy/docker-compose.yml config | Out-Host
    }

    Invoke-CheckedDockerCommand -Description "build server image" -Command {
        docker build -f server/Dockerfile -t $ImageTag . | Out-Host
    }

    Remove-ContainerIfPresent -Name $containerName

    Invoke-CheckedDockerCommand -Description "run hardened server container" -Command {
        docker run -d --rm `
            --name $containerName `
            --publish "${Port}:3000" `
            --read-only `
            --tmpfs /tmp `
            --cap-drop ALL `
            --security-opt no-new-privileges:true `
            --env RARENA_BIND=0.0.0.0:3000 `
            --env RARENA_LOG_FORMAT=json `
            --env RUST_LOG=info `
            --env RARENA_RECORD_STORE_PATH=/app/server/var/player_records.tsv `
            --env RARENA_WEB_CLIENT_ROOT=/app/server/static/webclient `
            --volume "${tempWebRoot}:/app/server/static/webclient:ro" `
            $ImageTag | Out-Host
    }

    Wait-ForHttpBody -Uri $healthUri -ExpectedBody "ok"

    $metrics = Invoke-WebRequest -Uri $metricsUri -UseBasicParsing -TimeoutSec 5
    if ($metrics.Content -notmatch "rarena_build_info") {
        throw "Prometheus metrics endpoint did not include rarena_build_info."
    }

    $root = Invoke-WebRequest -Uri $rootUri -UseBasicParsing -TimeoutSec 5
    if ($root.Content -notmatch "Rusaren Docker Smoke" -and $root.Content -notmatch "Rusaren Shell") {
        throw "Hosted root did not return either the placeholder web bundle or the exported shell."
    }

    Write-Host "Docker smoke succeeded against $ImageTag on port $Port."
}
catch {
    Write-Warning $_
    try {
        docker logs $containerName 2>$null | Out-Host
    }
    catch {
    }
    throw
}
finally {
    if (-not $KeepContainer) {
        Remove-ContainerIfPresent -Name $containerName
    }

    if (Test-Path $tempWebRoot) {
        Remove-Item -Recurse -Force -Path $tempWebRoot
    }
    Pop-Location
}
