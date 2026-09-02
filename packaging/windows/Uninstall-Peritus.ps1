#Requires -Version 5.1
[CmdletBinding()]
param(
    [string]$InstallRoot,
    [string]$DataRoot
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($InstallRoot) -or [string]::IsNullOrWhiteSpace($DataRoot)) {
    if ([string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) {
        throw 'LOCALAPPDATA is required when install and data roots are not supplied'
    }
}
$programRoot = if ([string]::IsNullOrWhiteSpace($InstallRoot)) {
    Join-Path $env:LOCALAPPDATA 'Programs\Peritus'
} else {
    [IO.Path]::GetFullPath($InstallRoot)
}
$dataRoot = if ([string]::IsNullOrWhiteSpace($DataRoot)) {
    Join-Path $env:LOCALAPPDATA 'Peritus'
} else {
    [IO.Path]::GetFullPath($DataRoot)
}
$taskFile = Join-Path $dataRoot 'supervisor\Peritus.Task.xml'

Stop-ScheduledTask -TaskName 'Peritus' -ErrorAction SilentlyContinue
Unregister-ScheduledTask -TaskName 'Peritus' -Confirm:$false -ErrorAction SilentlyContinue
Remove-Item -LiteralPath $taskFile -Force -ErrorAction SilentlyContinue
Remove-Item -LiteralPath $programRoot -Recurse -Force -ErrorAction SilentlyContinue

$binRoot = Join-Path $programRoot 'bin'
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$entries = @($userPath -split ';' | Where-Object {
    $_ -and -not [String]::Equals($_.TrimEnd('\'), $binRoot.TrimEnd('\'), [StringComparison]::OrdinalIgnoreCase)
})
[Environment]::SetEnvironmentVariable('Path', ($entries -join ';'), 'User')

Write-Output 'Peritus package files were removed; configuration, state, logs, and credentials were preserved'
