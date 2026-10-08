#Requires -Version 5.1
[CmdletBinding()]
param(
    [string]$InstallRoot,
    [string]$DataRoot,
    [switch]$StopOnly
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

function Stop-PeritusPackage {
    param([string]$ProgramRoot, [string]$DataRoot, [switch]$RemoveTask)

    $daemon = Join-Path $ProgramRoot 'bin\peritusd.exe'
    # A missing task is normal for launcher-owned daemons. Query failures are not.
    $tasks = @(Get-ScheduledTask -TaskPath '\' -ErrorAction Stop | Where-Object { $_.TaskName -eq 'Peritus' })
    foreach ($task in $tasks) {
        $owned = @($task.Actions | Where-Object {
            $_.Execute -and [String]::Equals($_.Execute.Trim('"'), $daemon, [StringComparison]::OrdinalIgnoreCase)
        })
        if ($owned.Count -eq 0) { continue }
        $configuration = $null
        if ($owned.Count -eq 1) {
            $argumentsProperty = $owned[0].PSObject.Properties['Arguments']
            if ($null -ne $argumentsProperty -and [string]$argumentsProperty.Value -match '^supervise --config "([^"]+)"$') {
                $configuration = $Matches[1]
            }
        }
        if ($null -ne $configuration) {
            & $daemon package-handoff --config $configuration
            if ($LASTEXITCODE -ne 0) {
                throw "Peritus daemon refused an exact durable handoff for $configuration."
            }
            # A clean daemon exit also ends its supervisor successfully. Stop the registered task
            # only after that exact owner has issued its receipt, preventing a new manual start.
            Stop-ScheduledTask -InputObject $task -ErrorAction Stop
        } else {
            Stop-ScheduledTask -InputObject $task -ErrorAction Stop
        }
        if ($RemoveTask) { Unregister-ScheduledTask -InputObject $task -Confirm:$false -ErrorAction Stop }
    }

    if (-not [string]::IsNullOrWhiteSpace($DataRoot)) {
        $marker = Join-Path $DataRoot 'State\daemon\applied-configuration'
        if (Test-Path -LiteralPath $marker -PathType Leaf) {
            $lines = @(Get-Content -LiteralPath $marker)
            if ($lines.Count -ne 3 -or $lines[0] -ne 'peritus-applied-daemon-v2' -or -not $lines[1].StartsWith('configuration=', [StringComparison]::Ordinal)) {
                throw 'Applied daemon configuration marker is malformed.'
            }
            $configuration = $lines[1].Substring('configuration='.Length)
            if (-not (Test-Path -LiteralPath $configuration -PathType Leaf)) {
                throw "Recorded daemon configuration does not exist: $configuration"
            }
            & $daemon package-handoff --config $configuration | Out-Null
            if ($LASTEXITCODE -ne 0) {
                throw "Peritus daemon refused an exact durable handoff for $configuration."
            }
        }
    }
}

Stop-PeritusPackage -ProgramRoot $programRoot -DataRoot $dataRoot -RemoveTask:(-not $StopOnly)
if ($StopOnly) { return }

# Do not remove PATH entries or report success if an external lock or access denial remains.
if (Test-Path -LiteralPath $taskFile) { Remove-Item -LiteralPath $taskFile -Force -ErrorAction Stop }
foreach ($path in @((Join-Path $programRoot 'bin'), (Join-Path $programRoot 'libexec'), (Join-Path $programRoot 'share'))) {
    if (Test-Path -LiteralPath $path) {
        $item = Get-Item -LiteralPath $path -Force
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            Remove-Item -LiteralPath $path -Force -ErrorAction Stop
        }
    }
}
if (Test-Path -LiteralPath $programRoot) { Remove-Item -LiteralPath $programRoot -Recurse -Force -ErrorAction Stop }
if (Test-Path -LiteralPath $programRoot) { throw "Peritus package files remain at $programRoot." }

$binRoot = Join-Path $programRoot 'bin'
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$entries = @($userPath -split ';' | Where-Object {
    $_ -and -not [String]::Equals($_.TrimEnd('\'), $binRoot.TrimEnd('\'), [StringComparison]::OrdinalIgnoreCase)
})
[Environment]::SetEnvironmentVariable('Path', ($entries -join ';'), 'User')

Write-Output 'Peritus package files were removed; configuration, state, logs, and credentials were preserved'
