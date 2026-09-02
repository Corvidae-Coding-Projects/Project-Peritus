#Requires -Version 5.1
[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repository = if ($env:PERITUS_REPOSITORY) { $env:PERITUS_REPOSITORY } else { 'Corvidae-Coding-Projects/Project-Peritus' }
$releaseBase = if ($env:PERITUS_RELEASE_BASE_URL) { $env:PERITUS_RELEASE_BASE_URL.TrimEnd('/') } else { "https://github.com/$repository/releases/download" }
$headers = @{ 'User-Agent' = 'peritus-installer' }
$version = $env:PERITUS_VERSION
if ([string]::IsNullOrWhiteSpace($version)) {
    $release = Invoke-RestMethod -Headers $headers -Uri "https://api.github.com/repos/$repository/releases/latest"
    $version = [string]$release.tag_name
}
if ($version -notmatch '^v[0-9]+\.[0-9]+\.[0-9]+$') { throw "release version is not a vMAJOR.MINOR.PATCH tag: $version" }

$architecture = switch ([Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()) {
    'X64' { 'x86_64' }
    'Arm64' { 'aarch64' }
    default { throw "unsupported architecture: $_" }
}
$asset = "peritus-windows-$architecture.zip"
$archiveUrl = "$releaseBase/$version/$asset"
$temporary = Join-Path ([IO.Path]::GetTempPath()) ("peritus-install-{0}" -f [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temporary | Out-Null

function Receive-PeritusFile {
    param([Parameter(Mandatory = $true)][string]$Uri, [Parameter(Mandatory = $true)][string]$Output)
    if ($Uri.StartsWith('file://', [StringComparison]::OrdinalIgnoreCase)) {
        Copy-Item -LiteralPath ([Uri]$Uri).LocalPath -Destination $Output
    } else {
        Invoke-WebRequest -Headers $headers -UseBasicParsing -Uri $Uri -OutFile $Output
    }
}

function Get-PeritusSha256Hex {
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

try {
    Write-Output "Downloading Peritus $version for windows/$architecture..."
    $archive = Join-Path $temporary $asset
    $checksum = "$archive.sha256"
    Receive-PeritusFile -Uri $archiveUrl -Output $archive
    Receive-PeritusFile -Uri "$archiveUrl.sha256" -Output $checksum
    $expected = (Get-Content -LiteralPath $checksum -Raw).Trim()
    if ($expected -notmatch '^[0-9a-fA-F]{64}$') { throw 'release checksum is malformed' }
    $actual = Get-PeritusSha256Hex -Path $archive
    if (-not [String]::Equals($actual, $expected, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'release archive checksum did not match'
    }
    Expand-Archive -LiteralPath $archive -DestinationPath $temporary
    $bundle = Join-Path $temporary "peritus-windows-$architecture"
    if (-not (Test-Path -LiteralPath $bundle -PathType Container)) {
        throw "release archive did not contain $bundle"
    }
    $installed = Join-Path $env:LOCALAPPDATA 'Programs\Peritus\bin\peritus.exe'
    if (Test-Path -LiteralPath $installed -PathType Leaf) {
        & (Join-Path $bundle 'Upgrade-Peritus.ps1') -BundleRoot $bundle
    } else {
        & (Join-Path $bundle 'Install-Peritus.ps1') -BundleRoot $bundle
    }
    Write-Output "Peritus $version is installed. Open a terminal and run: peritus"
} finally {
    Remove-Item -LiteralPath $temporary -Recurse -Force -ErrorAction SilentlyContinue
}
