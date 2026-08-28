#Requires -Version 5.1
[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$BundleRoot)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$bundle = [IO.Path]::GetFullPath($BundleRoot)
if (-not [IO.Path]::IsPathRooted($BundleRoot)) { throw 'package directory must be absolute' }
$programRoot = Join-Path $env:LOCALAPPDATA 'Programs\Peritus'
$binRoot = Join-Path $programRoot 'bin'
$helperRoot = Join-Path $programRoot 'libexec'
$shareRoot = Join-Path $programRoot 'share'
if (-not (Test-Path -LiteralPath (Join-Path $bundle 'manifest.toml') -PathType Leaf) -or -not (Test-Path -LiteralPath (Join-Path $bundle 'SHA256SUMS') -PathType Leaf)) { throw 'package manifest and SHA256SUMS are required' }

function Get-Sha256Hex {
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

foreach ($line in Get-Content -LiteralPath (Join-Path $bundle 'SHA256SUMS')) {
    if ($line -notmatch '^([0-9a-fA-F]{64})  ([A-Za-z0-9._/-]+)$') { throw 'SHA256SUMS contains a malformed line' }
    $candidate = Join-Path $bundle $Matches[2]
    if (-not (Test-Path -LiteralPath $candidate -PathType Leaf)) { throw "package artifact is missing: $($Matches[2])" }
    if ((Get-Sha256Hex -Path $candidate) -ne $Matches[1]) { throw "package checksum mismatch for $($Matches[2])" }
}

New-Item -ItemType Directory -Path $binRoot, $helperRoot, $shareRoot -Force | Out-Null
function Publish-PackageFile { param([string]$Source, [string]$Target); $temporary = "$Target.new.$PID"; Copy-Item -LiteralPath $Source -Destination $temporary -Force; Move-Item -LiteralPath $temporary -Destination $Target -Force }
Publish-PackageFile (Join-Path $bundle 'bin\peritusd.exe') (Join-Path $binRoot 'peritusd.exe')
Publish-PackageFile (Join-Path $bundle 'bin\peritus.exe') (Join-Path $binRoot 'peritus.exe')
Publish-PackageFile (Join-Path $bundle 'bin\peritus-tui.exe') (Join-Path $binRoot 'peritus-tui.exe')
Publish-PackageFile (Join-Path $bundle 'libexec\peritus-windows-sandbox-helper.exe') (Join-Path $helperRoot 'peritus-windows-sandbox-helper.exe')
Publish-PackageFile (Join-Path $bundle 'share\peritus\Peritus.Task.xml.in') (Join-Path $shareRoot 'Peritus.Task.xml.in')

$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$entries = @($userPath -split ';' | Where-Object { $_ })
if (-not ($entries | Where-Object { [String]::Equals($_.TrimEnd('\'), $binRoot.TrimEnd('\'), [StringComparison]::OrdinalIgnoreCase) })) {
    $nextPath = if ([string]::IsNullOrWhiteSpace($userPath)) { $binRoot } else { "$userPath;$binRoot" }
    [Environment]::SetEnvironmentVariable('Path', $nextPath, 'User')
}
$env:Path = "$binRoot;$env:Path"
Write-Output 'Peritus installed. Open a terminal and run: peritus'
