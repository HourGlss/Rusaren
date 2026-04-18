[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
if ($PSVersionTable.PSVersion.Major -ge 7) {
    $PSNativeCommandUseErrorActionPreference = $true
}

$serverRoot = Split-Path -Parent $PSScriptRoot
$repoRoot = Split-Path -Parent $serverRoot

$cargoBin = Join-Path $HOME ".cargo\\bin"
if (Test-Path $cargoBin) {
    $env:PATH = "$cargoBin$([System.IO.Path]::PathSeparator)$env:PATH"
}

Set-Location $repoRoot

$stagedBackendFiles = @(
    git diff --cached --name-only --diff-filter=ACMR |
        Where-Object {
            $_ -match '^server/(bin|crates|fuzz)/.*\.rs$' -or
            $_ -match '^server/(Cargo\.toml|Cargo\.lock|\.cargo/config\.toml)$'
        }
)

if ($stagedBackendFiles.Count -eq 0) {
    Write-Host "No staged backend Rust files matched the CI guard."
    return
}

Push-Location $serverRoot
try {
    rustup run stable cargo xlint
}
finally {
    Pop-Location
}

$miriSensitiveMatches = New-Object System.Collections.Generic.List[object]

foreach ($relativePath in $stagedBackendFiles | Where-Object { $_ -like '*.rs' }) {
    $absolutePath = Join-Path $repoRoot ($relativePath -replace '/', '\')
    if (-not (Test-Path $absolutePath)) {
        continue
    }

    $content = Get-Content -Path $absolutePath -Raw
    $looksLikeTestSource = $relativePath -match '/tests/' -or $relativePath -match '/tests\.rs$' -or $content -match '#\[cfg\(test\)\]'
    if (-not $looksLikeTestSource) {
        continue
    }

    $lines = $content -split "`r?`n"
    for ($index = 0; $index -lt $lines.Length; $index++) {
        $line = $lines[$index]
        if ($line -match 'SystemTime::now\s*\(' -or $line -match '(?:std::)?env::temp_dir\s*\(') {
            $miriSensitiveMatches.Add([pscustomobject]@{
                    Path = $relativePath
                    Line = $index + 1
                    Text = $line.Trim()
                })
        }
    }
}

if ($miriSensitiveMatches.Count -gt 0) {
    Write-Error @"
Miri-sensitive test helpers were detected in staged backend Rust files.
Avoid wall-clock and temp-dir helpers in test code; prefer deterministic counters under repo-local scratch paths.
$(
    ($miriSensitiveMatches | ForEach-Object {
            "- $($_.Path):$($_.Line) :: $($_.Text)"
        }) -join [Environment]::NewLine
)
"@
}
