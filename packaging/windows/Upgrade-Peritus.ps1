#Requires -Version 5.1
[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$BundleRoot)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$bundle = [IO.Path]::GetFullPath($BundleRoot)
$programRoot = Join-Path $env:LOCALAPPDATA 'Programs\Peritus'
$backup = Join-Path ([IO.Path]::GetTempPath()) ("peritus-upgrade-{0}" -f [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $backup | Out-Null
if (Test-Path -LiteralPath $programRoot) { Copy-Item -LiteralPath $programRoot -Destination (Join-Path $backup 'Program') -Recurse }
try {
    & (Join-Path $bundle 'Install-Peritus.ps1') -BundleRoot $bundle
} catch {
    Remove-Item -LiteralPath $programRoot -Recurse -Force -ErrorAction SilentlyContinue
    if (Test-Path -LiteralPath (Join-Path $backup 'Program')) { Copy-Item -LiteralPath (Join-Path $backup 'Program') -Destination $programRoot -Recurse }
    throw
} finally {
    Remove-Item -LiteralPath $backup -Recurse -Force -ErrorAction SilentlyContinue
}
