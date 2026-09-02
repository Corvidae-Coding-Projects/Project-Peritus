#Requires -Version 5.1
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$BundleRoot,
    [string]$InstallRoot
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$bundle = [IO.Path]::GetFullPath($BundleRoot)
if ([string]::IsNullOrWhiteSpace($InstallRoot)) {
    if ([string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) {
        throw 'LOCALAPPDATA is required when InstallRoot is not supplied'
    }
    $programRoot = Join-Path $env:LOCALAPPDATA 'Programs\Peritus'
} else {
    if (-not [IO.Path]::IsPathRooted($InstallRoot)) { throw 'install directory must be absolute' }
    $programRoot = [IO.Path]::GetFullPath($InstallRoot)
}
$backup = Join-Path ([IO.Path]::GetTempPath()) ("peritus-upgrade-{0}" -f [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $backup | Out-Null
if (Test-Path -LiteralPath $programRoot) { Copy-Item -LiteralPath $programRoot -Destination (Join-Path $backup 'Program') -Recurse }
try {
    & (Join-Path $bundle 'Install-Peritus.ps1') -BundleRoot $bundle -InstallRoot $programRoot
} catch {
    Remove-Item -LiteralPath $programRoot -Recurse -Force -ErrorAction SilentlyContinue
    if (Test-Path -LiteralPath (Join-Path $backup 'Program')) { Copy-Item -LiteralPath (Join-Path $backup 'Program') -Destination $programRoot -Recurse }
    throw
} finally {
    Remove-Item -LiteralPath $backup -Recurse -Force -ErrorAction SilentlyContinue
}
