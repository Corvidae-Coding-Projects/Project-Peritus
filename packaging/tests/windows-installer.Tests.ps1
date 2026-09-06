#Requires -Version 5.1
[CmdletBinding()]
param([string]$RepositoryRoot = (Join-Path $PSScriptRoot '../..'))

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$RepositoryRoot = [IO.Path]::GetFullPath($RepositoryRoot)
$bootstrap = Join-Path $RepositoryRoot 'install.ps1'
$native = Join-Path $RepositoryRoot 'packaging/windows/Install-Peritus.ps1'

# Import only the exact functions under test. Never invoke installer entry points or system packages.
foreach ($source in @($bootstrap, $native)) {
    $tokens = $null
    $parseErrors = $null
    $ast = [Management.Automation.Language.Parser]::ParseFile($source, [ref]$tokens, [ref]$parseErrors)
    if ($parseErrors.Count -ne 0) { throw "PowerShell parse errors in ${source}: $parseErrors" }
    foreach ($name in @('Get-PeritusSha256Hex', 'Test-PeritusArchive', 'Receive-PeritusFile', 'Install-SystemPackage')) {
        $node = $ast.Find({ param($item) $item -is [Management.Automation.Language.FunctionDefinitionAst] -and $item.Name -eq $name }, $true)
        if ($null -ne $node) { . ([scriptblock]::Create($node.Extent.Text)) }
    }
}

function Assert-That {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

function Assert-Fails {
    param([scriptblock]$Action, [string]$Label = 'operation')
    $failed = $false
    try { & $Action } catch { $failed = $true }
    Assert-That $failed "Expected $Label to fail."
}

$temporary = Join-Path ([IO.Path]::GetTempPath()) ('peritus-ps-tests-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temporary | Out-Null
# Windows PowerShell 5.1 does not load the ZipArchive types with FileSystem alone.
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
$package = 'peritus-windows-x86_64'

function New-TestArchive {
    param([string[]]$Entries, [bool]$SymbolicLink = $false)
    $path = Join-Path $temporary ([guid]::NewGuid().ToString('N') + '.zip')
    $zip = [IO.Compression.ZipFile]::Open($path, [IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($name in $Entries) {
            $entry = $zip.CreateEntry($name)
            if ($SymbolicLink) { $entry.ExternalAttributes = -1610612736 }
        }
    } finally {
        $zip.Dispose()
    }
    return $path
}

try {
    $file = Join-Path $temporary 'hash-input'
    [IO.File]::WriteAllText($file, 'abc')
    Assert-Fails { Receive-PeritusFile 'http://example.invalid/package.zip' (Join-Path $temporary 'must-not-exist') } 'insecure download'
    Assert-That ((Get-PeritusSha256Hex $file) -eq 'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad') 'SHA-256 differs.'
    $valid = New-TestArchive @("$package/", "$package/bin/", "$package/bin/peritus.exe")
    Test-PeritusArchive $valid $package
    foreach ($bad in @("../outside", "$package/../outside", "$package//bin/a", "$package///", "$package/bin/a:stream", "/$package/bin/a", "$package/bin/a.", "$package/bin/NUL.txt")) {
        $archive = New-TestArchive @($bad)
        Assert-Fails { Test-PeritusArchive $archive $package } $bad
    }
    $duplicate = New-TestArchive @("$package/bin/a", "$package/bin/A")
    Assert-Fails { Test-PeritusArchive $duplicate $package }
    $link = New-TestArchive @("$package/bin/a") $true
    Assert-Fails { Test-PeritusArchive $link $package }
    $empty = New-TestArchive @()
    Assert-Fails { Test-PeritusArchive $empty $package }

    # These mocks cannot reach WinGet. Verify exact package routing and exit-code propagation.
    function Get-Command { param($Name, $CommandType, $ErrorAction); return [pscustomobject]@{ Name = $Name } }
    $script:packageCalls = @()
    $script:packageExit = 0
    function winget.exe {
        $script:packageCalls += ,@($args)
        $global:LASTEXITCODE = $script:packageExit
    }
    $savedDependencyMode = $env:PERITUS_INSTALL_DEPS
    $savedPath = $env:Path
    try {
        $env:PERITUS_INSTALL_DEPS = '0'
        Assert-Fails { Install-SystemPackage 'Git.Git' }
        Assert-That ($script:packageCalls.Count -eq 0) 'Opt-out called WinGet.'
        $env:PERITUS_INSTALL_DEPS = '1'
        Install-SystemPackage 'Git.Git'
        Assert-That (($script:packageCalls[0] -join ' ') -eq 'install --id Git.Git --exact --source winget --silent --accept-package-agreements --accept-source-agreements --disable-interactivity') 'WinGet arguments differ.'
        $script:packageExit = 42
        Assert-Fails { Install-SystemPackage 'Git.Git' }
    } finally {
        $env:PERITUS_INSTALL_DEPS = $savedDependencyMode
        $env:Path = $savedPath
    }
    Write-Output 'Windows installer parsing, checksums, archive rejection, and dependency routing passed.'
} finally {
    Remove-Item -LiteralPath $temporary -Recurse -Force
}
