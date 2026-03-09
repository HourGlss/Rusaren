[CmdletBinding()]
param(
    [string]$GodotExecutable = "",
    [string]$ImageTag = "rusaren/server:play",
    [string]$ContainerName = "rusaren-play",
    [string]$DataVolume = "rarena-play-data",
    [int]$Port = 3000,
    [ValidateSet("direct", "docker")]
    [string]$Runtime = "direct",
    [switch]$SkipExport,
    [switch]$SkipBuild,
    [switch]$NoOpen,
    [switch]$TailLogs,
    [switch]$Stop
)

$ErrorActionPreference = "Stop"

$serverRoot = Split-Path -Parent $PSScriptRoot
$repoRoot = Split-Path -Parent $serverRoot
$baseUrl = "http://127.0.0.1:$Port/"
$healthUrl = "http://127.0.0.1:$Port/healthz"
$directPidPath = Join-Path $serverRoot "var\play-local.pid"
$directStdoutLog = Join-Path $serverRoot "var\play-local.stdout.log"
$directStderrLog = Join-Path $serverRoot "var\play-local.stderr.log"

function Invoke-CheckedCommand {
    param(
        [string]$Description,
        [scriptblock]$Command
    )

    Write-Host "==> $Description"
    & $Command
    if ($LASTEXITCODE -ne 0) {
        throw "Command failed during '$Description' with exit code $LASTEXITCODE."
    }
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

function Stop-DirectProcessIfPresent {
    if (-not (Test-Path $directPidPath)) {
        return
    }

    try {
        $rawPid = Get-Content $directPidPath -ErrorAction Stop
        $pidValue = [int]::Parse($rawPid[0])
        $process = Get-Process -Id $pidValue -ErrorAction SilentlyContinue
        if ($null -ne $process) {
            Stop-Process -Id $pidValue -Force
        }
    }
    catch {
    }
    finally {
        Remove-Item $directPidPath -ErrorAction SilentlyContinue
    }
}

function Wait-ForHealth {
    param(
        [string]$Uri,
        [int]$Attempts = 30
    )

    for ($attempt = 1; $attempt -le $Attempts; $attempt++) {
        try {
            $response = Invoke-WebRequest -Uri $Uri -UseBasicParsing -TimeoutSec 2
            if ($response.StatusCode -eq 200 -and $response.Content -eq "ok") {
                return
            }
        }
        catch {
        }

        Start-Sleep -Seconds 1
    }

    throw "Timed out waiting for $Uri to report healthy."
}

Push-Location $repoRoot
try {
    if ($Stop) {
        Remove-ContainerIfPresent -Name $ContainerName
        Stop-DirectProcessIfPresent
        Write-Host "Stopped local play processes."
        return
    }

    if (-not $SkipExport) {
        $exportArgs = @(
            "-NoProfile",
            "-ExecutionPolicy", "Bypass",
            "-File", (Join-Path $serverRoot "scripts\export-web-client.ps1"),
            "-InstallTemplates"
        )
        if (-not [string]::IsNullOrWhiteSpace($GodotExecutable)) {
            $exportArgs += @("-GodotExecutable", $GodotExecutable)
        }

        Invoke-CheckedCommand -Description "export Godot web client" -Command {
            powershell @exportArgs
        }
    }

    if (-not $SkipBuild -and $Runtime -eq "docker") {
        Invoke-CheckedCommand -Description "build local play image" -Command {
            docker build -f server/Dockerfile -t $ImageTag . | Out-Host
        }
    }

    if ($Runtime -eq "docker") {
        Stop-DirectProcessIfPresent
        Remove-ContainerIfPresent -Name $ContainerName

        Invoke-CheckedCommand -Description "start local play container" -Command {
            docker run --rm -d `
                --name $ContainerName `
                --publish "${Port}:3000" `
                --read-only `
                --tmpfs /tmp `
                --cap-drop ALL `
                --security-opt no-new-privileges:true `
                --env RARENA_BIND=0.0.0.0:3000 `
                --env RARENA_LOG_FORMAT=pretty `
                --env RUST_LOG=info `
                --env RARENA_RECORD_STORE_PATH=/app/server/var/player_records.tsv `
                --env RARENA_WEB_CLIENT_ROOT=/app/server/static/webclient `
                --volume "${DataVolume}:/app/server/var" `
                $ImageTag | Out-Host
        }
    }
    else {
        Remove-ContainerIfPresent -Name $ContainerName
        Stop-DirectProcessIfPresent
        New-Item -ItemType Directory -Force -Path (Join-Path $serverRoot "var") | Out-Null
        Remove-Item $directStdoutLog, $directStderrLog -ErrorAction SilentlyContinue

        Invoke-CheckedCommand -Description "start local play server directly on the host" -Command {
            $env:RARENA_BIND = "127.0.0.1:$Port"
            $env:RARENA_LOG_FORMAT = "pretty"
            $env:RUST_LOG = "info"
            $process = Start-Process `
                -FilePath "cargo" `
                -ArgumentList @("run", "-p", "dedicated_server", "--quiet") `
                -WorkingDirectory $serverRoot `
                -WindowStyle Hidden `
                -RedirectStandardOutput $directStdoutLog `
                -RedirectStandardError $directStderrLog `
                -PassThru
            Set-Content -Path $directPidPath -Value $process.Id
        }
    }

    Wait-ForHealth -Uri $healthUrl

    $rootResponse = Invoke-WebRequest -Uri $baseUrl -UseBasicParsing -TimeoutSec 5
    if ($rootResponse.Content -notmatch "Rusaren Shell") {
        throw "The hosted shell did not return the expected Rusaren Shell page."
    }

    Write-Host ""
    Write-Host "Local play is ready."
    Write-Host "Open: $baseUrl"
    Write-Host "Health: $healthUrl"
    if ($Runtime -eq "docker") {
        Write-Host "Logs : docker logs -f $ContainerName"
    }
    else {
        Write-Host "Logs :"
        Write-Host "  stdout -> $directStdoutLog"
        Write-Host "  stderr -> $directStderrLog"
    }
    Write-Host "Stop : powershell -NoProfile -ExecutionPolicy Bypass -File .\server\scripts\play-local.ps1 -Stop"
    Write-Host ""
    Write-Host "Suggested test flow:"
    Write-Host "1. Open $baseUrl in two browser tabs."
    Write-Host "2. Connect two different players."
    Write-Host "3. Create or join a lobby, choose teams, and ready up."
    Write-Host "4. Pick skills, move with WASD, aim with the mouse, and use left click plus 1-5 during combat."
    if ($Runtime -eq "direct") {
        Write-Host ""
        Write-Host "Direct runtime is the default because browser WebRTC works more reliably against a host-bound server than a local Docker NAT."
    }

    if (-not $NoOpen) {
        Start-Process $baseUrl
    }

    if ($TailLogs -and $Runtime -eq "docker") {
        docker logs -f $ContainerName
    }
}
catch {
    Write-Warning $_
    try {
        if ($Runtime -eq "docker") {
            docker logs $ContainerName 2>$null | Out-Host
        }
        else {
            if (Test-Path $directStdoutLog) {
                Get-Content $directStdoutLog | Out-Host
            }
            if (Test-Path $directStderrLog) {
                Get-Content $directStderrLog | Out-Host
            }
        }
    }
    catch {
    }
    throw
}
finally {
    Pop-Location
}
