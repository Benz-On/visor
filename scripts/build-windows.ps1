[CmdletBinding()]
param(
    [switch]$SkipInstall,
    [switch]$SkipTests
)

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $projectRoot

function Get-Sha256Hex {
    param([Parameter(Mandatory)][string]$LiteralPath)

    $stream = [System.IO.File]::OpenRead($LiteralPath)
    $algorithm = [System.Security.Cryptography.SHA256]::Create()
    try {
        return ([System.BitConverter]::ToString($algorithm.ComputeHash($stream))).Replace('-', '').ToLowerInvariant()
    }
    finally {
        $algorithm.Dispose()
        $stream.Dispose()
    }
}

if (-not $IsWindows -and $PSVersionTable.PSEdition -eq 'Core') {
    throw 'VISOR Windows artifacts must be built on Windows.'
}

$manifest = Get-Content -LiteralPath (Join-Path $projectRoot 'package.json') -Raw | ConvertFrom-Json
$version = [string]$manifest.version
$cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
$vswhere = 'C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe'

if (-not (Test-Path -LiteralPath (Join-Path $cargoBin 'cargo.exe'))) {
    throw 'Cargo was not found. Install Rust stable with the x86_64-pc-windows-msvc target.'
}
if (-not (Test-Path -LiteralPath $vswhere)) {
    throw 'Visual Studio Build Tools 2022 were not found.'
}

$visualStudio = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $visualStudio) {
    throw 'The Visual Studio C++ workload is required.'
}
$devCommand = Join-Path $visualStudio 'Common7\Tools\VsDevCmd.bat'

$steps = @(
    ('call "{0}" -no_logo -arch=x64' -f $devCommand),
    ('set "PATH={0};%PATH%"' -f $cargoBin)
)
if (-not $SkipInstall) {
    $steps += 'npm.cmd ci'
}
if (-not $SkipTests) {
    $steps += 'npm.cmd run lint'
    $steps += 'npm.cmd run test:agent'
    $steps += 'cargo test --manifest-path src-tauri\Cargo.toml'
}
$steps += 'cargo clean --manifest-path src-tauri\Cargo.toml --release -p visor'
$steps += 'npm.cmd exec tauri build -- --no-bundle --ci'

& cmd.exe /d /s /c ($steps -join ' && ')
if ($LASTEXITCODE -ne 0) {
    throw "VISOR Windows build failed with exit code $LASTEXITCODE."
}

$releaseRoot = Join-Path $projectRoot 'src-tauri\target\release'
$portableSource = Join-Path $releaseRoot 'visor.exe'
if (-not (Test-Path -LiteralPath $portableSource)) {
    throw 'The portable VISOR executable was not produced.'
}

$artifactRoot = Join-Path $projectRoot 'artifacts'
New-Item -ItemType Directory -Force -Path $artifactRoot | Out-Null
$portableName = "VISOR-$version-windows-x64-portable.exe"
$installerName = "VISOR-$version-windows-x64-setup.exe"
$portableTarget = Join-Path $artifactRoot $portableName
$installerTarget = Join-Path $artifactRoot $installerName
Copy-Item -LiteralPath $portableSource -Destination $portableTarget -Force

$bundleSteps = @(
    ('call "{0}" -no_logo -arch=x64' -f $devCommand),
    ('set "PATH={0};%PATH%"' -f $cargoBin),
    'npm.cmd exec tauri bundle -- --bundles nsis --ci'
)
& cmd.exe /d /s /c ($bundleSteps -join ' && ')
if ($LASTEXITCODE -ne 0) {
    throw "VISOR NSIS bundling failed with exit code $LASTEXITCODE."
}

$installerSource = Get-ChildItem -LiteralPath (Join-Path $releaseRoot 'bundle\nsis') -Filter '*setup.exe' |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1
if (-not $installerSource) {
    throw 'The NSIS installer was not produced.'
}
Copy-Item -LiteralPath $installerSource.FullName -Destination $installerTarget -Force

$files = @($portableTarget, $installerTarget) | ForEach-Object {
    $item = Get-Item -LiteralPath $_
    [pscustomobject][ordered]@{
        name = $item.Name
        bytes = $item.Length
        sha256 = Get-Sha256Hex -LiteralPath $_
    }
}
$releaseManifest = [ordered]@{
    product = 'VISOR'
    version = $version
    channel = if ($version.Contains('-')) { 'prerelease' } else { 'stable' }
    platform = 'windows'
    architecture = 'x64'
    signed = $false
    updater = 'disabled for pre-release artifacts'
    webview2 = [ordered]@{
        installer = 'embedded bootstrapper'
        portable = 'requires the Windows WebView2 runtime'
    }
    generatedAt = [DateTime]::UtcNow.ToString('o')
    files = $files
}
$releaseManifest | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $artifactRoot 'release-manifest.json') -Encoding UTF8
$files | ForEach-Object { "{0}  {1}" -f $_.sha256, $_.name } | Set-Content -LiteralPath (Join-Path $artifactRoot 'SHA256SUMS.txt') -Encoding ASCII

Write-Host "VISOR $version Windows artifacts are ready in $artifactRoot"
$files | Format-Table name, bytes, sha256 -AutoSize
