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
    param([string]$ProgramRoot, [switch]$RemoveTask)

    $daemon = Join-Path $ProgramRoot 'bin\peritusd.exe'
    # A missing task is normal for launcher-owned daemons. Query failures are not.
    $tasks = @(Get-ScheduledTask -TaskPath '\' -ErrorAction Stop | Where-Object { $_.TaskName -eq 'Peritus' })
    foreach ($task in $tasks) {
        $owned = @($task.Actions | Where-Object {
            $_.Execute -and [String]::Equals($_.Execute.Trim('"'), $daemon, [StringComparison]::OrdinalIgnoreCase)
        })
        if ($owned.Count -eq 0) { continue }
        Stop-ScheduledTask -InputObject $task -ErrorAction Stop
        if ($RemoveTask) { Unregister-ScheduledTask -InputObject $task -Confirm:$false -ErrorAction Stop }
    }

    # Stop launchers before their daemon/helpers, including processes not owned by a task.
    # Exact executable paths keep other installations and unrelated same-name processes alive.
    foreach ($relative in @('bin\peritus.exe', 'bin\peritus-tui.exe', 'bin\peritusd.exe', 'libexec\peritus-windows-sandbox-helper.exe')) {
        $executable = Join-Path $ProgramRoot $relative
        $name = [IO.Path]::GetFileNameWithoutExtension($executable)
        foreach ($process in @(Get-Process | Where-Object { $_.ProcessName -eq $name })) {
            try {
                if (-not [String]::Equals($process.Path, $executable, [StringComparison]::OrdinalIgnoreCase)) { continue }
                if (-not $process.HasExited) {
                    Write-Output "Stopping installed Peritus process: $($process.Id)"
                    try { $process.Kill() } catch { if (-not $process.HasExited) { throw } }
                    if (-not $process.WaitForExit(10000)) { throw "Installed Peritus process $($process.Id) did not stop within 10 seconds." }
                }
            } finally { $process.Dispose() }
        }
    }
}

Stop-PeritusPackage -ProgramRoot $programRoot -RemoveTask:(-not $StopOnly)
if ($StopOnly) { return }

# Do not remove PATH entries or report success if an external lock or access denial remains.
if (Test-Path -LiteralPath $taskFile) { Remove-Item -LiteralPath $taskFile -Force -ErrorAction Stop }
if (Test-Path -LiteralPath $programRoot) { Remove-Item -LiteralPath $programRoot -Recurse -Force -ErrorAction Stop }
if (Test-Path -LiteralPath $programRoot) { throw "Peritus package files remain at $programRoot." }

$binRoot = Join-Path $programRoot 'bin'
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$entries = @($userPath -split ';' | Where-Object {
    $_ -and -not [String]::Equals($_.TrimEnd('\'), $binRoot.TrimEnd('\'), [StringComparison]::OrdinalIgnoreCase)
})
[Environment]::SetEnvironmentVariable('Path', ($entries -join ';'), 'User')

Write-Output 'Peritus package files were removed; configuration, state, logs, and credentials were preserved'
