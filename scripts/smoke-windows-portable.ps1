[CmdletBinding()]
param(
    [string]$Path,
    [int]$StartupSeconds = 8
)

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $projectRoot

if (-not $Path) {
    $version = [string](Get-Content -LiteralPath (Join-Path $projectRoot 'package.json') -Raw | ConvertFrom-Json).version
    $Path = Join-Path $projectRoot "artifacts\VISOR-$version-windows-x64-portable.exe"
}
$resolvedPath = (Resolve-Path -LiteralPath $Path).Path
$process = Start-Process -FilePath $resolvedPath -PassThru -WindowStyle Hidden

try {
    Start-Sleep -Seconds $StartupSeconds
    $process.Refresh()
    if ($process.HasExited) {
        throw "VISOR exited during startup with code $($process.ExitCode)."
    }
    Write-Host "VISOR portable smoke test passed (PID $($process.Id), ${StartupSeconds}s alive)."
}
finally {
    if (-not $process.HasExited) {
        Stop-Process -Id $process.Id -Force
        $process.WaitForExit(5000) | Out-Null
    }
}
