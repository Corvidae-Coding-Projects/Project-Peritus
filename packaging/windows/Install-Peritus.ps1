#Requires -Version 5.1
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$BundleRoot
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$bundle = [IO.Path]::GetFullPath($BundleRoot)
if (-not [IO.Path]::IsPathRooted($BundleRoot)) {
    throw 'package directory must be absolute'
}
$programRoot = Join-Path $env:LOCALAPPDATA 'Programs\Peritus'
$binRoot = Join-Path $programRoot 'bin'
$helperRoot = Join-Path $programRoot 'libexec'
$dataRoot = Join-Path $env:LOCALAPPDATA 'Peritus'
$configFile = Join-Path $dataRoot 'config\peritus.toml'
$stateRoot = Join-Path $dataRoot 'state'
$logRoot = Join-Path $dataRoot 'logs'
$supervisorRoot = Join-Path $dataRoot 'supervisor'
$taskFile = Join-Path $supervisorRoot 'Peritus.Task.xml'

if (-not (Test-Path -LiteralPath $configFile -PathType Leaf)) {
    throw "operator-provisioned regular configuration is required at $configFile"
}
if (-not (Test-Path -LiteralPath (Join-Path $bundle 'manifest.toml') -PathType Leaf) -or
    -not (Test-Path -LiteralPath (Join-Path $bundle 'SHA256SUMS') -PathType Leaf)) {
    throw 'package manifest and SHA256SUMS are required'
}

function Assert-PackageChecksums {
    param([string]$Root)
    $rootPrefix = [IO.Path]::GetFullPath($Root).TrimEnd('\') + '\'
    foreach ($line in Get-Content -LiteralPath (Join-Path $Root 'SHA256SUMS')) {
        if ($line -notmatch '^([0-9a-fA-F]{64})  ([A-Za-z0-9._/-]+)$') {
            throw 'SHA256SUMS contains a malformed line'
        }
        $relative = $Matches[2].Replace('/', [IO.Path]::DirectorySeparatorChar)
        $candidate = [IO.Path]::GetFullPath((Join-Path $Root $relative))
        if (-not $candidate.StartsWith($rootPrefix, [StringComparison]::OrdinalIgnoreCase)) {
            throw 'SHA256SUMS path escapes the package directory'
        }
        $observed = (Get-FileHash -LiteralPath $candidate -Algorithm SHA256).Hash
        if ($observed -ne $Matches[1]) {
            throw "package checksum mismatch for $relative"
        }
    }
}

function Set-OwnerOnlyDirectory {
    param([string]$Path)
    New-Item -ItemType Directory -Path $Path -Force | Out-Null
    $grant = "{0}:(OI)(CI)F" -f [Security.Principal.WindowsIdentity]::GetCurrent().Name
    & icacls.exe $Path '/inheritance:r' '/grant:r' $grant '/c' | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "could not protect $Path" }
}

function Set-OwnerOnlyFile {
    param([string]$Path)
    $grant = "{0}:F" -f [Security.Principal.WindowsIdentity]::GetCurrent().Name
    & icacls.exe $Path '/inheritance:r' '/grant:r' $grant '/c' | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "could not protect $Path" }
}

function Publish-PackageFile {
    param([string]$Source, [string]$Target)
    $temporary = "$Target.new.$PID"
    Copy-Item -LiteralPath $Source -Destination $temporary -Force
    Move-Item -LiteralPath $temporary -Destination $Target -Force
}

Assert-PackageChecksums -Root $bundle
Set-OwnerOnlyDirectory -Path $programRoot
Set-OwnerOnlyDirectory -Path $binRoot
Set-OwnerOnlyDirectory -Path $helperRoot
Set-OwnerOnlyDirectory -Path (Split-Path -Parent $configFile)
Set-OwnerOnlyDirectory -Path $stateRoot
Set-OwnerOnlyDirectory -Path $logRoot
Set-OwnerOnlyDirectory -Path $supervisorRoot
Set-OwnerOnlyFile -Path $configFile

Publish-PackageFile (Join-Path $bundle 'bin\peritusd.exe') (Join-Path $binRoot 'peritusd.exe')
Publish-PackageFile (Join-Path $bundle 'bin\peritus.exe') (Join-Path $binRoot 'peritus.exe')
Publish-PackageFile (Join-Path $bundle 'bin\peritus-tui.exe') (Join-Path $binRoot 'peritus-tui.exe')
Publish-PackageFile (Join-Path $bundle 'libexec\peritus-windows-sandbox-helper.exe') (Join-Path $helperRoot 'peritus-windows-sandbox-helper.exe')

$template = Get-Content -LiteralPath (Join-Path $bundle 'share\peritus\Peritus.Task.xml.in') -Raw
$identity = [Security.Principal.WindowsIdentity]::GetCurrent().Name
$taskXml = $template.Replace('@USER_ID@', [Security.SecurityElement]::Escape($identity))
$taskXml = $taskXml.Replace('@PERITUSD@', [Security.SecurityElement]::Escape((Join-Path $binRoot 'peritusd.exe')))
$taskXml = $taskXml.Replace('@CONFIG_FILE@', [Security.SecurityElement]::Escape($configFile))
$taskXml = $taskXml.Replace('@WORKING_DIRECTORY@', [Security.SecurityElement]::Escape($env:USERPROFILE))
[IO.File]::WriteAllText($taskFile, $taskXml, [Text.UTF8Encoding]::new($false))

Register-ScheduledTask -TaskName 'Peritus' -Xml $taskXml -Force | Out-Null
Start-ScheduledTask -TaskName 'Peritus'

$ready = $false
for ($attempt = 0; $attempt -lt 30; $attempt++) {
    $instance = Join-Path $stateRoot 'daemon.instance'
    if (Test-Path -LiteralPath $instance -PathType Leaf) {
        $endpointLine = Get-Content -LiteralPath $instance | Where-Object { $_ -like 'endpoint=*' } | Select-Object -First 1
        if ($null -ne $endpointLine) {
            $endpoint = '\\.\pipe\' + $endpointLine.Substring(9)
            & (Join-Path $binRoot 'peritus.exe') --endpoint $endpoint status *> $null
            if ($LASTEXITCODE -eq 0) {
                $ready = $true
                break
            }
        }
    }
    Start-Sleep -Seconds 1
}

if (-not $ready) {
    throw 'peritusd did not publish an authenticated ready endpoint'
}
