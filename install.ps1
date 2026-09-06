#Requires -Version 5.1
[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') { throw 'Use install.sh on Linux and macOS.' }
if (-not [Environment]::Is64BitProcess) { throw 'Run this installer from 64-bit PowerShell.' }

$repository = if ($env:PERITUS_REPOSITORY) { $env:PERITUS_REPOSITORY } else { 'Corvidae-Coding-Projects/Project-Peritus' }
$releaseBase = if ($env:PERITUS_RELEASE_BASE_URL) { $env:PERITUS_RELEASE_BASE_URL.TrimEnd('/') } else { "https://github.com/$repository/releases/download" }
$headers = @{ 'User-Agent' = 'peritus-installer' }
$releaseTag = '@PERITUS_RELEASE_TAG@'
$version = if ($env:PERITUS_VERSION) { $env:PERITUS_VERSION } else { $releaseTag }
if ($version.StartsWith('@')) { $version = '' }
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
    $address = [Uri]$Uri
    if (-not $address.IsAbsoluteUri -or $address.Scheme -notin @('https', 'file')) {
        throw 'Release downloads require HTTPS or an explicit local file source.'
    }
    if ($address.Scheme -eq 'file') {
        Copy-Item -LiteralPath $address.LocalPath -Destination $Output
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

function Test-PeritusArchive {
    param([Parameter(Mandatory = $true)][string]$Archive, [Parameter(Mandatory = $true)][string]$PackageName)

    Add-Type -AssemblyName System.IO.Compression
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead($Archive)
    try {
        if ($zip.Entries.Count -eq 0 -or $zip.Entries.Count -gt 64) { throw 'Invalid package entry count.' }
        $names = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
        [long]$total = 0
        foreach ($entry in $zip.Entries) {
            $fullName = $entry.FullName.Replace('\', '/')
            if ($fullName.Contains('//')) { throw 'Release archive contains an unsafe path.' }
            $name = $fullName.TrimEnd('/')
            if ($name -ne $PackageName -and -not $name.StartsWith("$PackageName/", [StringComparison]::Ordinal)) {
                throw 'Release archive contains a path outside the package.'
            }
            $invalidSegments = @($name.Split('/') | Where-Object {
                $_ -in @('', '.', '..') -or $_.EndsWith('.') -or $_ -match '^(CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(\.|$)'
            })
            if ($name -notmatch '^[A-Za-z0-9._/-]+$' -or $invalidSegments.Count -ne 0) {
                throw 'Release archive contains an unsafe path.'
            }
            if (-not $names.Add($name)) { throw 'Release archive contains a duplicate path.' }
            $kind = ($entry.ExternalAttributes -shr 16) -band 0xF000
            if ($kind -notin @(0, 0x8000, 0x4000)) { throw 'Release archive contains a link or special file.' }
            $total += $entry.Length
            if ($total -gt 1073741824) { throw 'Expanded release archive exceeds 1 GiB.' }
        }
    } finally {
        $zip.Dispose()
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
    Test-PeritusArchive -Archive $archive -PackageName "peritus-windows-$architecture"
    Expand-Archive -LiteralPath $archive -DestinationPath $temporary
    $bundle = Join-Path $temporary "peritus-windows-$architecture"
    if (-not (Test-Path -LiteralPath $bundle -PathType Container)) {
        throw "release archive did not contain $bundle"
    }
    $installed = Join-Path $env:LOCALAPPDATA 'Programs\Peritus\bin\peritus.exe'
    $adapter = if (Test-Path -LiteralPath $installed -PathType Leaf) { 'Upgrade-Peritus.ps1' } else { 'Install-Peritus.ps1' }
    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $bundle $adapter) -BundleRoot $bundle
    if ($LASTEXITCODE -ne 0) { throw "Native installation failed (exit $LASTEXITCODE)." }
    $observed = & $installed --version
    if ($LASTEXITCODE -ne 0 -or $observed -ne "peritus $($version.Substring(1))") {
        throw "The installed version does not match $version."
    }
    Write-Output "Peritus $version is installed. Open a terminal and run: peritus"
} finally {
    Remove-Item -LiteralPath $temporary -Recurse -Force -ErrorAction SilentlyContinue
}
