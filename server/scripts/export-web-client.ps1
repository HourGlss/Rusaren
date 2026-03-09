[CmdletBinding()]
param(
    [string]$GodotExecutable = "",
    [string]$ProjectPath = "",
    [string]$OutputPath = "",
    [switch]$InstallTemplates,
    [switch]$DownloadPortable,
    [string]$GodotVersionTag = "4.6.1-stable",
    [string]$GodotInstallRoot = ""
)

$ErrorActionPreference = "Stop"

$serverRoot = Split-Path -Parent $PSScriptRoot
$repoRoot = Split-Path -Parent $serverRoot

if ([string]::IsNullOrWhiteSpace($ProjectPath)) {
    $ProjectPath = Join-Path $repoRoot "client\\godot"
}

if ([string]::IsNullOrWhiteSpace($OutputPath)) {
    $OutputPath = Join-Path $serverRoot "static\\webclient\\index.html"
}

if ([string]::IsNullOrWhiteSpace($GodotInstallRoot)) {
    $GodotInstallRoot = Join-Path $serverRoot "tools\\godot"
}

function Invoke-FileDownload {
    param(
        [string]$Url,
        [string]$OutFile
    )

    $curl = Get-Command curl.exe -ErrorAction SilentlyContinue
    if ($null -ne $curl) {
        & $curl.Source --location --fail --output $OutFile $Url
        return
    }

    Invoke-WebRequest -Uri $Url -OutFile $OutFile
}

function Get-GodotBuildInfo {
    param(
        [string]$ExecutablePath,
        [string]$FallbackVersionTag
    )

    $versionOutput = (& $ExecutablePath --version | Select-Object -First 1).Trim()
    if ($versionOutput -match "^(?<version>\d+\.\d+\.\d+)\.(?<channel>[A-Za-z0-9]+)") {
        return @{
            VersionText = [string]$Matches.version
            Channel = [string]$Matches.channel
            VersionTag = "{0}-{1}" -f $Matches.version, $Matches.channel
            TemplateDirName = "{0}.{1}" -f $Matches.version, $Matches.channel
        }
    }

    if ($FallbackVersionTag -notmatch "^(?<version>\d+\.\d+\.\d+)-(?<channel>[A-Za-z0-9]+)$") {
        throw "Unable to parse the Godot version output '$versionOutput' or fallback tag '$FallbackVersionTag'."
    }

    return @{
        VersionText = [string]$Matches.version
        Channel = [string]$Matches.channel
        VersionTag = $FallbackVersionTag
        TemplateDirName = "{0}.{1}" -f $Matches.version, $Matches.channel
    }
}

function Find-GodotExecutable {
    param(
        [string]$RequestedPath,
        [string]$InstallRoot,
        [string]$VersionTag,
        [switch]$AllowDownload
    )

    $candidates = @()
    if (-not [string]::IsNullOrWhiteSpace($RequestedPath)) {
        $candidates += $RequestedPath
    }
    $candidates += Join-Path $repoRoot "Godot\\Godot_v*.exe"
    $candidates += Join-Path $InstallRoot $VersionTag

    foreach ($candidate in $candidates) {
        if ($candidate.Contains("*")) {
            $resolved = Get-ChildItem -Path $candidate -File -ErrorAction SilentlyContinue |
                Sort-Object FullName |
                Select-Object -First 1
            if ($null -ne $resolved) {
                return $resolved.FullName
            }
            continue
        }

        if (Test-Path $candidate) {
            $item = Get-Item $candidate
            if ($item.PSIsContainer) {
                $resolved = Get-ChildItem -Path $candidate -Recurse -File -Filter "Godot*_console.exe" -ErrorAction SilentlyContinue |
                    Sort-Object FullName |
                    Select-Object -First 1
                if ($null -ne $resolved) {
                    return $resolved.FullName
                }
            }
            else {
                return $item.FullName
            }
        }
    }

    if ($AllowDownload) {
        return Install-PortableGodotEditor -InstallRoot $InstallRoot -VersionTag $VersionTag
    }

    throw "No Godot executable was found. Pass -GodotExecutable or rerun with -DownloadPortable."
}

function Install-PortableGodotEditor {
    param(
        [string]$InstallRoot,
        [string]$VersionTag
    )

    $versionRoot = Join-Path $InstallRoot $VersionTag
    $expectedBinary = Get-ChildItem -Path $versionRoot -Recurse -File -Filter "Godot*_console.exe" -ErrorAction SilentlyContinue |
        Sort-Object FullName |
        Select-Object -First 1
    if ($null -ne $expectedBinary) {
        return $expectedBinary.FullName
    }

    $tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("rusaren-godot-editor-" + [System.Guid]::NewGuid().ToString("N"))
    $archivePath = Join-Path $tempRoot "godot-editor.zip"
    $extractRoot = Join-Path $tempRoot "extract"
    $downloadUrl = "https://github.com/godotengine/godot-builds/releases/download/$VersionTag/Godot_v$VersionTag`_win64.exe.zip"

    New-Item -ItemType Directory -Force -Path $tempRoot | Out-Null
    New-Item -ItemType Directory -Force -Path $versionRoot | Out-Null

    Invoke-FileDownload -Url $downloadUrl -OutFile $archivePath
    Expand-Archive -Path $archivePath -DestinationPath $extractRoot
    Copy-Item -Path (Join-Path $extractRoot "*") -Destination $versionRoot -Recurse -Force

    $binary = Get-ChildItem -Path $versionRoot -Recurse -File -Filter "Godot*_console.exe" -ErrorAction SilentlyContinue |
        Sort-Object FullName |
        Select-Object -First 1
    if ($null -eq $binary) {
        throw "Portable Godot editor archive did not contain a console executable."
    }

    return $binary.FullName
}

function Resolve-TemplatePayloadRoot {
    param([string]$ExtractRoot)

    $templatesDirectory = Get-ChildItem -Path $ExtractRoot -Recurse -Directory -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -eq "templates" } |
        Select-Object -First 1
    if ($null -ne $templatesDirectory) {
        return $templatesDirectory.FullName
    }

    $webTemplate = Get-ChildItem -Path $ExtractRoot -Recurse -File -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -like "web_*" } |
        Select-Object -First 1
    if ($null -ne $webTemplate) {
        return $webTemplate.DirectoryName
    }

    $firstDirectory = Get-ChildItem -Path $ExtractRoot -Directory -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if ($null -ne $firstDirectory) {
        return $firstDirectory.FullName
    }

    return $ExtractRoot
}

function Ensure-GodotExportTemplates {
    param(
        [hashtable]$BuildInfo,
        [switch]$AllowInstall
    )

    $templateRoot = Join-Path $env:APPDATA "Godot\\export_templates\\$($BuildInfo.TemplateDirName)"
    $existingTemplate = Get-ChildItem -Path $templateRoot -Recurse -File -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if ($null -ne $existingTemplate) {
        return $templateRoot
    }

    if (-not $AllowInstall) {
        throw "Godot export templates were not found at '$templateRoot'. Rerun with -InstallTemplates."
    }

    $tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("rusaren-godot-templates-" + [System.Guid]::NewGuid().ToString("N"))
    $archivePath = Join-Path $tempRoot "godot-templates.zip"
    $extractRoot = Join-Path $tempRoot "extract"
    $downloadUrl = "https://github.com/godotengine/godot-builds/releases/download/$($BuildInfo.VersionTag)/Godot_v$($BuildInfo.VersionTag)`_export_templates.tpz"

    New-Item -ItemType Directory -Force -Path $tempRoot | Out-Null
    New-Item -ItemType Directory -Force -Path $templateRoot | Out-Null

    Invoke-FileDownload -Url $downloadUrl -OutFile $archivePath
    Expand-Archive -Path $archivePath -DestinationPath $extractRoot

    $payloadRoot = Resolve-TemplatePayloadRoot -ExtractRoot $extractRoot
    Copy-Item -Path (Join-Path $payloadRoot "*") -Destination $templateRoot -Recurse -Force
    return $templateRoot
}

function Clear-WebExportRoot {
    param([string]$OutputFilePath)

    $outputRoot = Split-Path -Parent $OutputFilePath
    if (Test-Path $outputRoot) {
        Remove-Item -Path (Join-Path $outputRoot "*") -Recurse -Force -ErrorAction SilentlyContinue
    }
    else {
        New-Item -ItemType Directory -Force -Path $outputRoot | Out-Null
    }
}

function Assert-WebExportArtifacts {
    param([string]$OutputFilePath)

    if (-not (Test-Path $OutputFilePath)) {
        throw "Godot export did not produce '$OutputFilePath'."
    }

    $outputRoot = Split-Path -Parent $OutputFilePath
    $jsArtifact = Get-ChildItem -Path $outputRoot -Filter *.js -File -ErrorAction SilentlyContinue | Select-Object -First 1
    $wasmArtifact = Get-ChildItem -Path $outputRoot -Filter *.wasm -File -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($null -eq $jsArtifact -or $null -eq $wasmArtifact) {
        throw "Godot export did not produce the expected JavaScript and WebAssembly artifacts in '$outputRoot'."
    }
}

function Sync-ProjectWebRtcExtension {
    param(
        [string]$ProjectRoot,
        [string]$ExecutablePath
    )

    $destinationRoot = Join-Path $ProjectRoot "webrtc"
    $candidateRoots = @(
        (Join-Path $repoRoot "Godot\webrtc"),
        (Join-Path (Split-Path -Parent $ExecutablePath) "webrtc"),
        (Join-Path (Split-Path -Parent (Split-Path -Parent $ExecutablePath)) "webrtc")
    ) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }

    $sourceRoot = $candidateRoots |
        Where-Object { Test-Path (Join-Path $_ "webrtc.gdextension") } |
        Select-Object -First 1

    if ([string]::IsNullOrWhiteSpace($sourceRoot)) {
        Write-Host "No local Godot WebRTC extension bundle was found. Native/headless WebRTC checks will use the browser-first fallback path."
        return
    }

    if (Test-Path $destinationRoot) {
        Remove-Item -Recurse -Force -Path $destinationRoot
    }

    New-Item -ItemType Directory -Force -Path $destinationRoot | Out-Null
    Copy-Item -Path (Join-Path $sourceRoot "*") -Destination $destinationRoot -Recurse -Force
    Write-Host "Synced Godot WebRTC extension from '$sourceRoot' to '$destinationRoot'."
}

$resolvedGodotExecutable = Find-GodotExecutable `
    -RequestedPath $GodotExecutable `
    -InstallRoot $GodotInstallRoot `
    -VersionTag $GodotVersionTag `
    -AllowDownload:$DownloadPortable

$buildInfo = Get-GodotBuildInfo -ExecutablePath $resolvedGodotExecutable -FallbackVersionTag $GodotVersionTag
[void](Ensure-GodotExportTemplates -BuildInfo $buildInfo -AllowInstall:$InstallTemplates)
Sync-ProjectWebRtcExtension -ProjectRoot $ProjectPath -ExecutablePath $resolvedGodotExecutable
Clear-WebExportRoot -OutputFilePath $OutputPath

& $resolvedGodotExecutable --headless --path $ProjectPath --export-release "Web" $OutputPath | Out-Host
Assert-WebExportArtifacts -OutputFilePath $OutputPath

Write-Host "Godot Web export complete:"
Write-Host "  Project: $ProjectPath"
Write-Host "  Output : $OutputPath"
Write-Host "  Godot  : $resolvedGodotExecutable"
