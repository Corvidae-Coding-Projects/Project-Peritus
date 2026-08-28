#Requires -Version 5.1
[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$programRoot = Join-Path $env:LOCALAPPDATA 'Programs\Peritus'
$dataRoot = Join-Path $env:LOCALAPPDATA 'Peritus'
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
