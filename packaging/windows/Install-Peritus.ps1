#Requires -Version 5.1
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$BundleRoot,
    [string]$InstallRoot
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$bundle = [IO.Path]::GetFullPath($BundleRoot)
if (-not [IO.Path]::IsPathRooted($BundleRoot)) { throw 'package directory must be absolute' }
if ([string]::IsNullOrWhiteSpace($InstallRoot)) {
    if ([string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) {
        throw 'LOCALAPPDATA is required when InstallRoot is not supplied'
    }
    $programRoot = Join-Path $env:LOCALAPPDATA 'Programs\Peritus'
} else {
    if (-not [IO.Path]::IsPathRooted($InstallRoot)) { throw 'install directory must be absolute' }
    $programRoot = [IO.Path]::GetFullPath($InstallRoot)
}
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

function Install-SystemPackage {
    param([Parameter(Mandatory = $true)][string]$Id)

    if ($env:PERITUS_INSTALL_DEPS -eq '0') {
        throw "Install $Id, then retry. PERITUS_INSTALL_DEPS=0 prevents automatic dependency installation."
    }
    if (-not (Get-Command winget.exe -CommandType Application -ErrorAction SilentlyContinue)) {
        throw 'Windows App Installer (WinGet) is required. Install App Installer from Microsoft Store, then retry.'
    }
    Write-Output "Installing runtime dependency: $Id"
    & winget.exe install --id $Id --exact --source winget --silent --accept-package-agreements --accept-source-agreements --disable-interactivity
    if ($LASTEXITCODE -ne 0 -and $LASTEXITCODE -ne 3010) {
        throw "Dependency installation failed for $Id (exit $LASTEXITCODE)."
    }
    $env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' + [Environment]::GetEnvironmentVariable('Path', 'User') + ';' + $env:Path
}

if ($env:PERITUS_INSTALL_DEPS -and $env:PERITUS_INSTALL_DEPS -notin @('0', '1')) {
    throw 'PERITUS_INSTALL_DEPS must be 0 or 1'
}
if (-not [Environment]::Is64BitProcess) { throw 'Run this installer from 64-bit PowerShell.' }
if (-not (Get-Command git.exe -CommandType Application -ErrorAction SilentlyContinue)) {
    Install-SystemPackage -Id 'Git.Git'
}
$runtime = Join-Path ([Environment]::SystemDirectory) 'vcruntime140.dll'
if (-not (Test-Path -LiteralPath $runtime -PathType Leaf)) {
    $runtimeArchitecture = switch ([Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()) {
        'X64' { 'x64' }
        'Arm64' { 'arm64' }
        default { throw "unsupported architecture: $_" }
    }
    Install-SystemPackage -Id "Microsoft.VCRedist.2015+.$runtimeArchitecture"
}
if (-not (Get-Command git.exe -CommandType Application -ErrorAction SilentlyContinue)) {
    throw 'Git installation did not provide git.exe. Open a new terminal and retry.'
}
& git.exe --version | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Git does not run.' }
& (Join-Path $bundle 'bin\peritus.exe') --version | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'The package cannot run. Check the Visual C++ runtime and Windows version.' }

New-Item -ItemType Directory -Path $binRoot, $helperRoot, $shareRoot -Force | Out-Null
function Publish-PackageFile { param([string]$Source, [string]$Target); $temporary = "$Target.new.$PID"; Copy-Item -LiteralPath $Source -Destination $temporary -Force; Move-Item -LiteralPath $temporary -Destination $Target -Force }
Publish-PackageFile (Join-Path $bundle 'bin\peritusd.exe') (Join-Path $binRoot 'peritusd.exe')
Publish-PackageFile (Join-Path $bundle 'bin\peritus.exe') (Join-Path $binRoot 'peritus.exe')
Publish-PackageFile (Join-Path $bundle 'bin\peritus-tui.exe') (Join-Path $binRoot 'peritus-tui.exe')
Publish-PackageFile (Join-Path $bundle 'libexec\peritus-windows-sandbox-helper.exe') (Join-Path $helperRoot 'peritus-windows-sandbox-helper.exe')
Publish-PackageFile (Join-Path $bundle 'share\peritus\Peritus.Task.xml.in') (Join-Path $shareRoot 'Peritus.Task.xml.in')
Publish-PackageFile (Join-Path $bundle 'Uninstall-Peritus.ps1') (Join-Path $shareRoot 'Uninstall-Peritus.ps1')

$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$entries = @($userPath -split ';' | Where-Object { $_ })
if (-not ($entries | Where-Object { [String]::Equals($_.TrimEnd('\'), $binRoot.TrimEnd('\'), [StringComparison]::OrdinalIgnoreCase) })) {
    $nextPath = if ([string]::IsNullOrWhiteSpace($userPath)) { $binRoot } else { "$userPath;$binRoot" }
    [Environment]::SetEnvironmentVariable('Path', $nextPath, 'User')
}
$env:Path = "$binRoot;$env:Path"
Write-Output 'Peritus installed. Open a terminal and run: peritus'
