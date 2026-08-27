#Requires -Version 5.1
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$BundleRoot
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$bundle = [IO.Path]::GetFullPath($BundleRoot)
$programRoot = Join-Path $env:LOCALAPPDATA 'Programs\Peritus'
$dataRoot = Join-Path $env:LOCALAPPDATA 'Peritus'
$taskFile = Join-Path $dataRoot 'supervisor\Peritus.Task.xml'
$backup = Join-Path ([IO.Path]::GetTempPath()) ("peritus-upgrade-{0}" -f [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $backup | Out-Null

if (Test-Path -LiteralPath $programRoot) {
    Copy-Item -LiteralPath $programRoot -Destination (Join-Path $backup 'Program') -Recurse
}
if (Test-Path -LiteralPath $taskFile -PathType Leaf) {
    Copy-Item -LiteralPath $taskFile -Destination (Join-Path $backup 'Peritus.Task.xml')
}

try {
    Stop-ScheduledTask -TaskName 'Peritus' -ErrorAction SilentlyContinue
    & (Join-Path $bundle 'Install-Peritus.ps1') -BundleRoot $bundle
} catch {
    Stop-ScheduledTask -TaskName 'Peritus' -ErrorAction SilentlyContinue
    Unregister-ScheduledTask -TaskName 'Peritus' -Confirm:$false -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $programRoot -Recurse -Force -ErrorAction SilentlyContinue
    if (Test-Path -LiteralPath (Join-Path $backup 'Program')) {
        Copy-Item -LiteralPath (Join-Path $backup 'Program') -Destination $programRoot -Recurse
    }
    $savedTask = Join-Path $backup 'Peritus.Task.xml'
    if (Test-Path -LiteralPath $savedTask -PathType Leaf) {
        Copy-Item -LiteralPath $savedTask -Destination $taskFile -Force
        $taskXml = Get-Content -LiteralPath $taskFile -Raw
        Register-ScheduledTask -TaskName 'Peritus' -Xml $taskXml -Force | Out-Null
        Start-ScheduledTask -TaskName 'Peritus'
    }
    throw
} finally {
    Remove-Item -LiteralPath $backup -Recurse -Force -ErrorAction SilentlyContinue
}
