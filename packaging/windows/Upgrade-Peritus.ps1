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
& (Join-Path $bundle 'Install-Peritus.ps1') -BundleRoot $bundle -InstallRoot $programRoot
