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

$installStore = Join-Path $programRoot '.install'
$generationRoot = Join-Path $installStore 'generations'
$transactionRoot = Join-Path $installStore 'transactions'
$receiptRoot = Join-Path $installStore 'receipts'
$current = Join-Path $installStore 'current'
$active = Join-Path $transactionRoot 'active'
$migration = Join-Path $transactionRoot 'migration'
New-Item -ItemType Directory -Path $programRoot, $installStore, $generationRoot, $transactionRoot, $receiptRoot -Force | Out-Null
if ($env:PERITUS_INSTALL_LOCKED -ne '1') {
    $powerShell = Join-Path $PSHOME 'powershell.exe'
    & (Join-Path $bundle 'bin\peritusd.exe') package-lock --lock (Join-Path $transactionRoot 'install.lock') -- $powerShell -NoProfile -ExecutionPolicy Bypass -File $PSCommandPath -BundleRoot $bundle -InstallRoot $programRoot
    exit $LASTEXITCODE
}
$generation = (Get-Sha256Hex -Path (Join-Path $bundle 'SHA256SUMS')).ToLowerInvariant()

function Write-DurableRecord {
    param([Parameter(Mandatory = $true)][string]$Path, [Parameter(Mandatory = $true)][string[]]$Lines)

    $temporary = "$Path.new.$PID"
    $encoding = New-Object Text.UTF8Encoding($false)
    $stream = New-Object IO.FileStream($temporary, [IO.FileMode]::Create, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try {
        $writer = New-Object IO.StreamWriter($stream, $encoding)
        try { foreach ($line in $Lines) { $writer.WriteLine($line) }; $writer.Flush(); $stream.Flush($true) }
        finally { $writer.Dispose() }
    } finally { $stream.Dispose() }
    if (Test-Path -LiteralPath $Path -PathType Leaf) { [IO.File]::Replace($temporary, $Path, $null) }
    else { [IO.File]::Move($temporary, $Path) }
}

function Get-ReceiptField {
    param([Parameter(Mandatory = $true)][string]$Path, [Parameter(Mandatory = $true)][string]$Name)

    $values = @(Get-Content -LiteralPath $Path | Where-Object { $_.StartsWith("$Name=", [StringComparison]::Ordinal) } | ForEach-Object { $_.Substring($Name.Length + 1) })
    if ($values.Count -ne 1 -or [string]::IsNullOrWhiteSpace($values[0])) { throw "Durable install receipt has an invalid $Name field." }
    return $values[0]
}

function Test-RetainedGeneration {
    param([Parameter(Mandatory = $true)][string]$Identifier)

    $root = Join-Path $generationRoot $Identifier
    $source = Join-Path $root '.source-sha256'
    if (-not (Test-Path -LiteralPath $source -PathType Leaf) -or ([IO.File]::ReadAllText($source).Trim() -ne $Identifier)) { return $false }
    foreach ($relative in @('bin\peritusd.exe', 'bin\peritus.exe', 'bin\peritus-tui.exe', 'libexec\peritus-windows-sandbox-helper.exe', 'share\peritus\Peritus.Task.xml.in', 'Uninstall-Peritus.ps1', 'stable-share\Peritus.Task.xml.in', 'stable-share\Uninstall-Peritus.ps1')) {
        if (-not (Test-Path -LiteralPath (Join-Path $root $relative) -PathType Leaf)) { return $false }
    }
    if ($Identifier.StartsWith('legacy-', [StringComparison]::Ordinal)) { return $true }
    foreach ($line in Get-Content -LiteralPath (Join-Path $root 'SHA256SUMS')) {
        if ($line -notmatch '^([0-9a-fA-F]{64})  ([A-Za-z0-9._/-]+)$') { return $false }
        $candidate = Join-Path $root $Matches[2]
        if (-not (Test-Path -LiteralPath $candidate -PathType Leaf) -or (Get-Sha256Hex -Path $candidate) -ne $Matches[1]) { return $false }
    }
    return $true
}

function Stage-Generation {
    param([Parameter(Mandatory = $true)][string]$Identifier)

    $destination = Join-Path $generationRoot $Identifier
    if (Test-Path -LiteralPath $destination -PathType Container) {
        if (-not (Test-RetainedGeneration -Identifier $Identifier)) { throw "Retained generation $Identifier is incomplete." }
        return
    }
    $temporary = Join-Path $generationRoot ".stage-$Identifier-$PID"
    if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary -Recurse -Force }
    New-Item -ItemType Directory -Path (Join-Path $temporary 'bin'), (Join-Path $temporary 'libexec'), (Join-Path $temporary 'share\peritus'), (Join-Path $temporary 'stable-share') -Force | Out-Null
    foreach ($relative in @('bin\peritusd.exe', 'bin\peritus.exe', 'bin\peritus-tui.exe', 'libexec\peritus-windows-sandbox-helper.exe', 'share\peritus\Peritus.Task.xml.in', 'Install-Peritus.ps1', 'Upgrade-Peritus.ps1', 'Uninstall-Peritus.ps1', 'manifest.toml', 'SHA256SUMS')) {
        Copy-Item -LiteralPath (Join-Path $bundle $relative) -Destination (Join-Path $temporary $relative)
    }
    Copy-Item -LiteralPath (Join-Path $bundle 'share\peritus\Peritus.Task.xml.in') -Destination (Join-Path $temporary 'stable-share\Peritus.Task.xml.in')
    Copy-Item -LiteralPath (Join-Path $bundle 'Uninstall-Peritus.ps1') -Destination (Join-Path $temporary 'stable-share\Uninstall-Peritus.ps1')
    [IO.File]::WriteAllText((Join-Path $temporary '.source-sha256'), "$Identifier`n", (New-Object Text.UTF8Encoding($false)))
    [IO.Directory]::Move($temporary, $destination)
    if (-not (Test-RetainedGeneration -Identifier $Identifier)) { throw "Staged generation $Identifier failed verification." }
}

function New-GenerationJunction {
    param([Parameter(Mandatory = $true)][string]$Path, [Parameter(Mandatory = $true)][string]$Target)

    if (Test-Path -LiteralPath $Path) { Remove-Item -LiteralPath $Path -Force }
    New-Item -ItemType Junction -Path $Path -Target $Target | Out-Null
}

function Set-AtomicLink {
    param(
        [Parameter(Mandatory = $true)][string]$Link,
        [Parameter(Mandatory = $true)][string]$Target,
        [string]$Configuration,
        [string]$Retired
    )

    $candidate = Join-Path (Split-Path -Parent $Link) ('.current.' + [guid]::NewGuid().ToString('N'))
    New-GenerationJunction -Path $candidate -Target $Target
    try {
        $arguments = @('package-adopt', '--store', $installStore, '--candidate', $candidate, '--current', $Link)
        if (-not [string]::IsNullOrWhiteSpace($Configuration)) { $arguments += @('--config', $Configuration) }
        if (-not [string]::IsNullOrWhiteSpace($Retired)) { $arguments += @('--retired', $Retired) }
        & (Join-Path $bundle 'bin\peritusd.exe') @arguments
        if ($LASTEXITCODE -ne 0) { throw "Atomic generation-link adoption failed for $Link." }
    } finally {
        if (Test-Path -LiteralPath $candidate) { Remove-Item -LiteralPath $candidate -Force -ErrorAction Stop }
    }
}

function Set-CurrentGeneration {
    param(
        [Parameter(Mandatory = $true)][string]$Identifier,
        [AllowNull()][string]$Configuration
    )

    Set-AtomicLink -Link $current -Target (Join-Path $generationRoot $Identifier) -Configuration $Configuration
}

function Set-StableJunctions {
    Set-AtomicLink -Link $binRoot -Target (Join-Path $current 'bin')
    Set-AtomicLink -Link $helperRoot -Target (Join-Path $current 'libexec')
    Set-AtomicLink -Link $shareRoot -Target (Join-Path $current 'stable-share')
}

function Test-StableJunctions {
    foreach ($relative in @('bin\peritusd.exe', 'bin\peritus.exe', 'bin\peritus-tui.exe', 'libexec\peritus-windows-sandbox-helper.exe', 'share\Peritus.Task.xml.in', 'share\Uninstall-Peritus.ps1')) {
        if (-not (Test-Path -LiteralPath (Join-Path $programRoot $relative) -PathType Leaf)) { return $false }
    }
    return $true
}

function Get-HandoffConfiguration {
    $marker = if ([string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) { $null } else { Join-Path $env:LOCALAPPDATA 'Peritus\State\daemon\applied-configuration' }
    if ($null -ne $marker -and (Test-Path -LiteralPath $marker -PathType Leaf)) {
        $lines = @(Get-Content -LiteralPath $marker)
        if ($lines.Count -ne 3 -or $lines[0] -ne 'peritus-applied-daemon-v2' -or -not $lines[1].StartsWith('configuration=', [StringComparison]::Ordinal)) { throw 'Applied daemon configuration marker is malformed.' }
        return $lines[1].Substring('configuration='.Length)
    }
    if (-not [string]::IsNullOrWhiteSpace($env:PERITUS_DAEMON_CONFIG)) { return [IO.Path]::GetFullPath($env:PERITUS_DAEMON_CONFIG) }
    $installedDaemon = Join-Path $programRoot 'bin\peritusd.exe'
    foreach ($task in @(Get-ScheduledTask -TaskPath '\' -ErrorAction Stop | Where-Object { $_.TaskName -eq 'Peritus' })) {
        foreach ($action in @($task.Actions)) {
            if ($action.Execute -and [String]::Equals($action.Execute.Trim('"'), $installedDaemon, [StringComparison]::OrdinalIgnoreCase) -and [string]$action.Arguments -match '^supervise --config "([^"]+)"$') { return $Matches[1] }
        }
    }
    return $null
}

function Get-TransactionConfiguration {
    $configuration = Get-HandoffConfiguration
    if ([string]::IsNullOrWhiteSpace($configuration)) { return 'none' }
    if ($configuration.Contains("`r") -or $configuration.Contains("`n")) { throw 'Daemon configuration path contains a line break.' }
    $configuration = [IO.Path]::GetFullPath($configuration)
    if (-not (Test-Path -LiteralPath $configuration -PathType Leaf)) { throw "Recorded daemon configuration does not exist: $configuration" }
    return $configuration
}

function Get-PersistedConfiguration {
    param([Parameter(Mandatory = $true)][string]$Path)

    $configuration = Get-ReceiptField -Path $Path -Name 'configuration'
    if ($configuration -eq 'none') { return $null }
    if (-not [IO.Path]::IsPathRooted($configuration) -or
        -not [String]::Equals([IO.Path]::GetFullPath($configuration), $configuration, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Durable install receipt has a non-absolute configuration.'
    }
    if (-not (Test-Path -LiteralPath $configuration -PathType Leaf)) { throw "Recorded daemon configuration does not exist: $configuration" }
    return $configuration
}

function Invoke-DurableHandoff {
    param(
        [Parameter(Mandatory = $true)][string]$Receipt,
        [AllowNull()][string]$Configuration,
        [string]$Daemon
    )

    $installedDaemon = if ([string]::IsNullOrWhiteSpace($Daemon)) { Join-Path $programRoot 'bin\peritusd.exe' } else { $Daemon }
    if (-not (Test-Path -LiteralPath $installedDaemon -PathType Leaf)) {
        Write-DurableRecord -Path $Receipt -Lines @('peritus-package-handoff-v1', 'status=not-installed')
        return
    }
    if ([string]::IsNullOrWhiteSpace($Configuration)) { throw 'Existing installation has no discoverable daemon configuration. Set PERITUS_DAEMON_CONFIG and retry.' }
    $temporary = "$Receipt.new.$PID"
    & $installedDaemon package-handoff --config $Configuration | Set-Content -LiteralPath $temporary -Encoding Ascii
    if ($LASTEXITCODE -ne 0) {
        if ($LASTEXITCODE -ne 2) {
            Move-Item -LiteralPath $temporary -Destination "$Receipt.failed" -Force
            throw 'The installed daemon rejected the exact durable handoff.'
        }
        & (Join-Path $bundle 'bin\peritusd.exe') package-handoff-legacy --config $Configuration | Set-Content -LiteralPath $temporary -Encoding Ascii
        if ($LASTEXITCODE -ne 0) {
            Move-Item -LiteralPath $temporary -Destination "$Receipt.failed" -Force
            throw 'The installed daemon could not complete an exact durable handoff. Stop the older client cleanly and retry.'
        }
    }
    $lines = @(Get-Content -LiteralPath $temporary)
    $status = @($lines | Where-Object { $_.StartsWith('status=', [StringComparison]::Ordinal) })
    if ($lines.Count -eq 0 -or $lines[0] -ne 'peritus-package-handoff-v1' -or $status.Count -ne 1 -or $status[0] -notin @('status=clean', 'status=already-stopped')) {
        Move-Item -LiteralPath $temporary -Destination "$Receipt.failed" -Force
        throw 'The installed daemon returned an invalid handoff receipt.'
    }
    if ($status[0] -eq 'status=already-stopped' -and (Test-Path -LiteralPath $Receipt -PathType Leaf)) {
        Remove-Item -LiteralPath $temporary -Force
    } elseif (Test-Path -LiteralPath $Receipt -PathType Leaf) {
        [IO.File]::Replace($temporary, $Receipt, $null)
    } else {
        [IO.File]::Move($temporary, $Receipt)
    }
}

function Stage-LegacyGeneration {
    foreach ($relative in @('bin\peritusd.exe', 'bin\peritus.exe', 'bin\peritus-tui.exe', 'libexec\peritus-windows-sandbox-helper.exe', 'share\Peritus.Task.xml.in', 'share\Uninstall-Peritus.ps1')) {
        if (-not (Test-Path -LiteralPath (Join-Path $programRoot $relative) -PathType Leaf)) { throw 'Existing package layout is partial.' }
    }
    $material = (@('bin\peritusd.exe', 'bin\peritus.exe', 'bin\peritus-tui.exe', 'libexec\peritus-windows-sandbox-helper.exe', 'share\Peritus.Task.xml.in', 'share\Uninstall-Peritus.ps1') | ForEach-Object { Get-Sha256Hex -Path (Join-Path $programRoot $_) }) -join "`n"
    $algorithm = [Security.Cryptography.SHA256]::Create()
    try { $legacyDigest = [BitConverter]::ToString($algorithm.ComputeHash([Text.Encoding]::UTF8.GetBytes($material))).Replace('-', '').ToLowerInvariant() }
    finally { $algorithm.Dispose() }
    $legacy = "legacy-$legacyDigest"
    $destination = Join-Path $generationRoot $legacy
    if (-not (Test-Path -LiteralPath $destination -PathType Container)) {
        $temporary = Join-Path $generationRoot ".stage-$legacy-$PID"
        New-Item -ItemType Directory -Path (Join-Path $temporary 'bin'), (Join-Path $temporary 'libexec'), (Join-Path $temporary 'share\peritus'), (Join-Path $temporary 'stable-share') -Force | Out-Null
        foreach ($name in @('peritusd.exe', 'peritus.exe', 'peritus-tui.exe')) { Copy-Item -LiteralPath (Join-Path $binRoot $name) -Destination (Join-Path $temporary "bin\$name") }
        Copy-Item -LiteralPath (Join-Path $helperRoot 'peritus-windows-sandbox-helper.exe') -Destination (Join-Path $temporary 'libexec\peritus-windows-sandbox-helper.exe')
        Copy-Item -LiteralPath (Join-Path $shareRoot 'Peritus.Task.xml.in') -Destination (Join-Path $temporary 'share\peritus\Peritus.Task.xml.in')
        Copy-Item -LiteralPath (Join-Path $shareRoot 'Uninstall-Peritus.ps1') -Destination (Join-Path $temporary 'Uninstall-Peritus.ps1')
        Copy-Item -LiteralPath (Join-Path $shareRoot 'Peritus.Task.xml.in') -Destination (Join-Path $temporary 'stable-share\Peritus.Task.xml.in')
        Copy-Item -LiteralPath (Join-Path $shareRoot 'Uninstall-Peritus.ps1') -Destination (Join-Path $temporary 'stable-share\Uninstall-Peritus.ps1')
        [IO.File]::WriteAllText((Join-Path $temporary '.source-sha256'), "$legacy`n", (New-Object Text.UTF8Encoding($false)))
        [IO.Directory]::Move($temporary, $destination)
    }
    $configuration = Get-TransactionConfiguration
    if ($configuration -eq 'none') { throw 'Existing installation has no discoverable daemon configuration. Set PERITUS_DAEMON_CONFIG and retry.' }
    Write-DurableRecord -Path $migration -Lines @('version=1', "legacy=$legacy", 'phase=staged', "configuration=$configuration", "handoff=handoff-$legacy.receipt")
}

function Resume-LegacyMigration {
    $legacy = Get-ReceiptField -Path $migration -Name 'legacy'
    $phase = Get-ReceiptField -Path $migration -Name 'phase'
    $configurationRecord = Get-ReceiptField -Path $migration -Name 'configuration'
    $configuration = Get-PersistedConfiguration -Path $migration
    if ($legacy -notmatch '^legacy-[0-9a-f]{64}$' -or -not (Test-RetainedGeneration -Identifier $legacy)) { throw 'Legacy recovery generation is invalid.' }
    if ($null -eq $configuration) { throw 'Legacy migration has no daemon configuration for the publication guard.' }
    $handoffReceipt = Join-Path $transactionRoot "handoff-$legacy.receipt"
    if ($phase -eq 'staged') {
        Invoke-DurableHandoff -Receipt $handoffReceipt -Configuration $configuration
        Write-DurableRecord -Path $migration -Lines @('version=1', "legacy=$legacy", 'phase=handoff', "configuration=$configurationRecord", "handoff=handoff-$legacy.receipt")
        $phase = 'handoff'
    }
    if ($phase -eq 'handoff') {
        Invoke-DurableHandoff -Receipt $handoffReceipt -Configuration $configuration
        Set-CurrentGeneration -Identifier $legacy -Configuration $configuration
        Write-DurableRecord -Path $migration -Lines @('version=1', "legacy=$legacy", 'phase=current', "configuration=$configurationRecord", "handoff=handoff-$legacy.receipt")
        $phase = 'current'
    }
    if ($phase -eq 'current') {
        Invoke-DurableHandoff -Receipt $handoffReceipt -Configuration $configuration
        foreach ($entry in @(
            @{ Path = $binRoot; Target = (Join-Path $current 'bin'); Name = 'bin' },
            @{ Path = $helperRoot; Target = (Join-Path $current 'libexec'); Name = 'libexec' },
            @{ Path = $shareRoot; Target = (Join-Path $current 'stable-share'); Name = 'share' }
        )) {
            $retired = Join-Path $transactionRoot "retired-$legacy-$($entry.Name)"
            Set-AtomicLink -Link $entry.Path -Target $entry.Target -Configuration $configuration -Retired $retired
        }
        Write-DurableRecord -Path $migration -Lines @('version=1', "legacy=$legacy", 'phase=linked', "configuration=$configurationRecord", "handoff=handoff-$legacy.receipt")
        $phase = 'linked'
    }
    if ($phase -ne 'linked') { throw 'Legacy migration receipt has an unknown phase.' }
    if (-not (Test-StableJunctions)) { throw 'Stable package junction migration did not verify.' }
    Write-DurableRecord -Path (Join-Path $receiptRoot "migration-$legacy.receipt") -Lines @('version=1', "legacy=$legacy", 'phase=verified', "configuration=$configurationRecord", "handoff=handoff-$legacy.receipt")
    Write-DurableRecord -Path (Join-Path $receiptRoot "handoff-$legacy.receipt") -Lines @(Get-Content -LiteralPath $handoffReceipt)
    foreach ($name in @('bin', 'libexec', 'share')) {
        $retired = Join-Path $transactionRoot "retired-$legacy-$name"
        if (Test-Path -LiteralPath $retired) { Remove-Item -LiteralPath $retired -Recurse -Force }
    }
    Remove-Item -LiteralPath $migration, $handoffReceipt -Force
}

if (Test-Path -LiteralPath $migration -PathType Leaf) {
    Resume-LegacyMigration
} elseif (-not (Test-Path -LiteralPath $current)) {
    if (Test-Path -LiteralPath (Join-Path $programRoot 'bin\peritusd.exe') -PathType Leaf) { Stage-LegacyGeneration; Resume-LegacyMigration }
}

function Write-ActiveTransaction {
    param([string]$Phase, [string]$Target, [string]$Previous, [string]$Configuration)
    Write-DurableRecord -Path $active -Lines @('version=1', "target=$Target", "previous=$Previous", "phase=$Phase", "configuration=$Configuration", "handoff=handoff-$Target.receipt")
}

function Restore-PreviousGeneration {
    param(
        [string]$Target,
        [string]$Previous,
        [string]$ConfigurationRecord,
        [AllowNull()][string]$Configuration
    )
    if ($null -ne $Configuration) {
        $targetDaemon = Join-Path $generationRoot "$Target\bin\peritusd.exe"
        Invoke-DurableHandoff -Receipt (Join-Path $transactionRoot "handoff-$Target.receipt") -Configuration $Configuration -Daemon $targetDaemon
    }
    if ($Previous -eq 'none') {
        $arguments = @('package-remove', '--store', $installStore, '--current', $current, '--expected', (Join-Path $generationRoot $Target))
        if ($null -ne $Configuration) { $arguments += @('--config', $Configuration) }
        & (Join-Path $bundle 'bin\peritusd.exe') @arguments
        if ($LASTEXITCODE -ne 0) { throw 'Unverified generation could not be removed.' }
        foreach ($path in @($binRoot, $helperRoot, $shareRoot)) {
            $item = Get-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
            if ($null -ne $item) {
                if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -eq 0) { throw "Refusing to remove a non-junction package path: $path" }
                Remove-Item -LiteralPath $path -Force
            }
        }
    } else {
        if (-not (Test-RetainedGeneration -Identifier $Previous)) { throw "Rollback generation $Previous is incomplete." }
        Set-CurrentGeneration -Identifier $Previous -Configuration $Configuration
        Set-StableJunctions
        if (-not (Test-StableJunctions)) { throw 'Rollback stable junctions did not verify.' }
    }
    Write-DurableRecord -Path (Join-Path $receiptRoot "$Target.rollback") -Lines @('version=1', "target=$Target", "previous=$Previous", 'phase=rolled-back', "configuration=$ConfigurationRecord")
    Remove-Item -LiteralPath $active -Force
}

function Resume-ActiveTransaction {
    $target = Get-ReceiptField -Path $active -Name 'target'
    $previous = Get-ReceiptField -Path $active -Name 'previous'
    $phase = Get-ReceiptField -Path $active -Name 'phase'
    $configurationRecord = Get-ReceiptField -Path $active -Name 'configuration'
    $configuration = Get-PersistedConfiguration -Path $active
    if ($target -notmatch '^[0-9a-f]{64}$' -or ($previous -ne 'none' -and $previous -notmatch '^(legacy-)?[0-9a-f]{64}$')) { throw 'Durable install receipt has a malformed generation identity.' }
    if ($previous -ne 'none' -and $null -eq $configuration) { throw 'Existing generation has no daemon configuration for the publication guard.' }
    if ($phase -eq 'rollback') { Restore-PreviousGeneration -Target $target -Previous $previous -ConfigurationRecord $configurationRecord -Configuration $configuration; return }
    if (-not (Test-RetainedGeneration -Identifier $target)) { throw "Staged generation $target is incomplete." }
    $handoffReceipt = Join-Path $transactionRoot "handoff-$target.receipt"
    if ($phase -eq 'staged') { Invoke-DurableHandoff -Receipt $handoffReceipt -Configuration $configuration; Write-ActiveTransaction -Phase 'handoff' -Target $target -Previous $previous -Configuration $configurationRecord; $phase = 'handoff' }
    if ($phase -eq 'handoff') { Invoke-DurableHandoff -Receipt $handoffReceipt -Configuration $configuration; Set-CurrentGeneration -Identifier $target -Configuration $configuration; Write-ActiveTransaction -Phase 'adopted' -Target $target -Previous $previous -Configuration $configurationRecord; $phase = 'adopted' }
    if ($phase -ne 'adopted') { throw 'Durable install receipt has an unknown phase.' }
    Set-StableJunctions
    if (-not (Test-RetainedGeneration -Identifier $target) -or -not (Test-StableJunctions) -or ([IO.File]::ReadAllText((Join-Path $current '.source-sha256')).Trim() -ne $target)) {
        Write-ActiveTransaction -Phase 'rollback' -Target $target -Previous $previous -Configuration $configurationRecord
        Restore-PreviousGeneration -Target $target -Previous $previous -ConfigurationRecord $configurationRecord -Configuration $configuration
        throw 'New generation verification failed; the prior generation was restored.'
    }
    Write-DurableRecord -Path (Join-Path $receiptRoot "$target.receipt") -Lines @('version=1', "target=$target", "previous=$previous", 'phase=verified', "configuration=$configurationRecord", "handoff=handoff-$target.receipt")
    Write-DurableRecord -Path (Join-Path $receiptRoot "handoff-$target.receipt") -Lines @(Get-Content -LiteralPath $handoffReceipt)
    Remove-Item -LiteralPath $active, $handoffReceipt -Force
}

if (Test-Path -LiteralPath $active -PathType Leaf) { Resume-ActiveTransaction }
Stage-Generation -Identifier $generation
if ((Test-Path -LiteralPath $current) -and ([IO.File]::ReadAllText((Join-Path $current '.source-sha256')).Trim() -eq $generation)) {
    Set-StableJunctions
    if (-not (Test-RetainedGeneration -Identifier $generation) -or -not (Test-StableJunctions)) { throw 'Installed generation does not match the verified package.' }
} else {
    $previous = if (Test-Path -LiteralPath $current) { [IO.File]::ReadAllText((Join-Path $current '.source-sha256')).Trim() } else { 'none' }
    $configuration = Get-TransactionConfiguration
    if ($previous -ne 'none' -and $configuration -eq 'none') { throw 'Existing generation has no daemon configuration for the publication guard.' }
    Write-ActiveTransaction -Phase 'staged' -Target $generation -Previous $previous -Configuration $configuration
    Resume-ActiveTransaction
}

$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$entries = @($userPath -split ';' | Where-Object { $_ })
if (-not ($entries | Where-Object { [String]::Equals($_.TrimEnd('\'), $binRoot.TrimEnd('\'), [StringComparison]::OrdinalIgnoreCase) })) {
    $nextPath = if ([string]::IsNullOrWhiteSpace($userPath)) { $binRoot } else { "$userPath;$binRoot" }
    [Environment]::SetEnvironmentVariable('Path', $nextPath, 'User')
}
$env:Path = "$binRoot;$env:Path"
Write-Output 'Peritus installed. Open a terminal and run: peritus'
