#Requires -Version 5.1
[CmdletBinding()]
param([string]$RepositoryRoot = (Join-Path $PSScriptRoot '../..'))

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$RepositoryRoot = [IO.Path]::GetFullPath($RepositoryRoot)
$temporary = Join-Path ([IO.Path]::GetTempPath()) ('peritus-lifecycle-' + [guid]::NewGuid().ToString('N'))
$bundle = Join-Path $temporary 'bundle'
$program = Join-Path $temporary 'installed'
$data = Join-Path $temporary 'data'
$other = Join-Path $temporary 'other'
$savedUserPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$savedPath = $env:Path
$savedDeps = $env:PERITUS_INSTALL_DEPS
$script:children = @()
$failures = @()

function Assert-That {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

function Get-FixtureSha256Hex {
    param([Parameter(Mandatory = $true)][string]$Path)

    $algorithm = [Security.Cryptography.SHA256]::Create()
    try {
        $stream = [IO.File]::OpenRead($Path)
        try {
            return [BitConverter]::ToString($algorithm.ComputeHash($stream)).Replace('-', '')
        } finally {
            $stream.Dispose()
        }
    } finally {
        $algorithm.Dispose()
    }
}

function Start-FixtureProcess {
    param([string]$Executable)
    $ready = Join-Path $temporary ([guid]::NewGuid().ToString('N') + '.ready')
    $child = Start-Process -FilePath $Executable -ArgumentList ('"' + $ready + '"') -PassThru
    $script:children += $child
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    while (-not (Test-Path -LiteralPath $ready)) {
        if ($child.HasExited -or [DateTime]::UtcNow -ge $deadline) { throw 'Fixture process did not become ready.' }
        Start-Sleep -Milliseconds 50
    }
    return $child
}

function Stop-FixtureProcesses {
    foreach ($child in $script:children) {
        if (-not $child.HasExited) { $child.Kill(); $child.WaitForExit() }
        $child.Dispose()
    }
    $script:children = @()
}

function Test-TaskOwnership {
    # Exercise the exact task-routing function without touching the runner's scheduled tasks.
    $tokens = $null
    $errors = $null
    $ast = [Management.Automation.Language.Parser]::ParseFile((Join-Path $bundle 'Uninstall-Peritus.ps1'), [ref]$tokens, [ref]$errors)
    Assert-That ($errors.Count -eq 0) 'Uninstaller did not parse.'
    $node = $ast.Find({ param($item) $item -is [Management.Automation.Language.FunctionDefinitionAst] -and $item.Name -eq 'Stop-PeritusPackage' }, $true)
    Assert-That ($null -ne $node) 'Uninstaller has no shared process lifecycle function.'
    . ([scriptblock]::Create($node.Extent.Text))
    $script:taskCalls = @()
    $script:queryFails = $false
    $script:testTasks = @(
        [pscustomobject]@{ TaskName = 'Peritus'; Actions = @([pscustomobject]@{ Execute = (Join-Path $program 'bin/peritusd.exe') }) },
        [pscustomobject]@{ TaskName = 'Peritus'; Actions = @([pscustomobject]@{ Execute = (Join-Path $other 'peritusd.exe') }) }
    )
    function Get-ScheduledTask {
        param($TaskPath, $ErrorAction)
        Assert-That ($TaskPath -eq '\') 'Task lookup was not confined to the root task folder.'
        if ($script:queryFails) { throw 'fixture task query failure' }
        return $script:testTasks
    }
    function Get-Process { return @() }
    function Stop-ScheduledTask { param($InputObject, $ErrorAction); $script:taskCalls += "stop:$($InputObject.Actions[0].Execute)" }
    function Unregister-ScheduledTask { param($InputObject, $Confirm, $ErrorAction); $script:taskCalls += "remove:$($InputObject.Actions[0].Execute)" }
    $daemon = Join-Path $program 'bin/peritusd.exe'
    Stop-PeritusPackage -ProgramRoot $program
    Assert-That (($script:taskCalls -join ';') -eq "stop:$daemon") 'Installation removed a task or stopped another installation.'
    $script:taskCalls = @()
    Stop-PeritusPackage -ProgramRoot $program -RemoveTask
    Assert-That (($script:taskCalls -join ';') -eq "stop:$daemon;remove:$daemon") 'Uninstall task routing was not scoped to the installation.'
    $script:queryFails = $true
    $rejected = $false
    try { Stop-PeritusPackage -ProgramRoot $program } catch { $rejected = $true }
    Assert-That $rejected 'Task query failure was silently ignored.'
    Write-Output 'PASS scheduled-task ownership and query failure'
}

try {
    New-Item -ItemType Directory -Path (Join-Path $bundle 'bin'), (Join-Path $bundle 'libexec'), (Join-Path $bundle 'share/peritus'), $other, $data -Force | Out-Null
    $source = Join-Path $temporary 'Fixture.cs'
    [IO.File]::WriteAllText($source, @'
using System;
using System.IO;
using System.Threading;
public static class Fixture {
    public static void Main(string[] args) {
        if (args.Length == 1 && args[0] == "--version") { Console.WriteLine("Peritus lifecycle fixture"); return; }
        File.WriteAllText(args[0], "ready");
        Thread.Sleep(300000);
    }
}
'@)
    $executable = Join-Path $bundle 'bin/peritus.exe'
    & (Join-Path $env:WINDIR 'Microsoft.NET/Framework64/v4.0.30319/csc.exe') /nologo /target:exe "/out:$executable" $source
    if ($LASTEXITCODE -ne 0) { throw 'Failed to compile the native lifecycle fixture.' }
    foreach ($relative in @('bin/peritusd.exe', 'bin/peritus-tui.exe', 'libexec/peritus-windows-sandbox-helper.exe')) {
        Copy-Item -LiteralPath $executable -Destination (Join-Path $bundle $relative)
    }
    Copy-Item -LiteralPath $executable -Destination (Join-Path $other 'peritusd.exe')
    foreach ($name in @('Install-Peritus.ps1', 'Uninstall-Peritus.ps1', 'Upgrade-Peritus.ps1')) {
        Copy-Item -LiteralPath (Join-Path $RepositoryRoot "packaging/windows/$name") -Destination $bundle
    }
    Copy-Item -LiteralPath (Join-Path $RepositoryRoot 'packaging/windows/Peritus.Task.xml.in') -Destination (Join-Path $bundle 'share/peritus')
    [IO.File]::WriteAllText((Join-Path $bundle 'manifest.toml'), 'schema = 1')
    $checksums = @(Get-ChildItem -LiteralPath $bundle -Recurse -File | ForEach-Object {
        $relative = $_.FullName.Substring($bundle.Length + 1).Replace('\', '/')
        (Get-FixtureSha256Hex -Path $_.FullName) + '  ' + $relative
    })
    [IO.File]::WriteAllLines((Join-Path $bundle 'SHA256SUMS'), $checksums)
    [IO.File]::WriteAllText((Join-Path $data 'state.json'), 'preserved user state')
    $env:PERITUS_INSTALL_DEPS = '0'
    $install = Join-Path $bundle 'Install-Peritus.ps1'
    $uninstall = Join-Path $bundle 'Uninstall-Peritus.ps1'

    foreach ($scenario in @('repeat-install', 'running-upgrade', 'running-uninstall', 'locked-install', 'locked-uninstall')) {
        try {
            & $install -BundleRoot $bundle -InstallRoot $program
            $unrelated = Start-FixtureProcess (Join-Path $other 'peritusd.exe')
            if ($scenario.StartsWith('locked-')) {
                $locked = [IO.File]::Open((Join-Path $program 'bin/peritusd.exe'), 'Open', 'Read', 'Read')
                try {
                    $pathBefore = [Environment]::GetEnvironmentVariable('Path', 'User')
                    $rejected = $false
                    try {
                        if ($scenario -eq 'locked-install') { & $install -BundleRoot $bundle -InstallRoot $program }
                        else { & $uninstall -InstallRoot $program -DataRoot $data }
                    } catch { $rejected = $true }
                    Assert-That $rejected 'Lifecycle action reported success while package files remained locked.'
                    Assert-That ([Environment]::GetEnvironmentVariable('Path', 'User') -eq $pathBefore) 'Failed lifecycle action changed PATH.'
                    Assert-That (@(Get-ChildItem -LiteralPath $program -Filter '*.new.*' -Recurse).Count -eq 0) 'Failed publication left staged files.'
                } finally { $locked.Dispose() }
                & $install -BundleRoot $bundle -InstallRoot $program
            } else {
                $running = @('bin/peritus.exe', 'bin/peritus-tui.exe', 'bin/peritusd.exe', 'libexec/peritus-windows-sandbox-helper.exe') | ForEach-Object {
                    Start-FixtureProcess (Join-Path $program $_)
                }
                if ($scenario -in @('repeat-install', 'running-upgrade')) {
                    $action = if ($scenario -eq 'running-upgrade') { Join-Path $bundle 'Upgrade-Peritus.ps1' } else { $install }
                    & $action -BundleRoot $bundle -InstallRoot $program
                    foreach ($child in $running) { Assert-That $child.HasExited 'Reinstall left an installed process running.' }
                    Assert-That ((Get-FixtureSha256Hex -Path (Join-Path $program 'bin/peritusd.exe')) -eq (Get-FixtureSha256Hex -Path (Join-Path $bundle 'bin/peritusd.exe'))) 'Reinstall published different bytes.'
                }
            }
            & $uninstall -InstallRoot $program -DataRoot $data
            Assert-That (-not (Test-Path -LiteralPath $program)) 'Uninstaller reported success but package files remain.'
            Assert-That (-not $unrelated.HasExited) 'Lifecycle action stopped another installation.'
            Assert-That ([IO.File]::ReadAllText((Join-Path $data 'state.json')) -eq 'preserved user state') 'Lifecycle action changed user data.'
            & $uninstall -InstallRoot $program -DataRoot $data
            Write-Output "PASS $scenario"
        } catch {
            $failures += "$scenario : $_"
            Write-Output "FAIL $scenario : $_"
        } finally {
            Stop-FixtureProcesses
            if (Test-Path -LiteralPath $program) { Remove-Item -LiteralPath $program -Recurse -Force }
        }
    }
    Test-TaskOwnership
    if ($failures.Count -gt 0) { throw ($failures -join "`n") }
    Write-Output 'Native Windows repeat installation, running uninstall, locked-file failure, and isolation passed.'
} finally {
    Stop-FixtureProcesses
    [Environment]::SetEnvironmentVariable('Path', $savedUserPath, 'User')
    $env:Path = $savedPath
    $env:PERITUS_INSTALL_DEPS = $savedDeps
    if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary -Recurse -Force }
}
