# Installation

No public release is available yet. Use [Install from source](#install-from-source) until the first release is published.

## System requirements

| System | Required version | Processors |
| --- | --- | --- |
| Linux | glibc 2.39 or later; kernel 6.6 or later | x86-64, ARM64 |
| macOS | macOS 15 or later | Intel, Apple silicon |
| Windows | Windows 11 24H2 or later, or Windows Server 2025 | x86-64, ARM64 |

Linux must provide Landlock, seccomp, and user namespaces for task execution.
Containers and restricted hosts can lack these features.
The installer does not replace your kernel or disable a global security restriction.
On Ubuntu, it can install the distribution's bubblewrap AppArmor profile when sandbox creation fails.

Use a terminal with interactive input. Direct API providers need an available, unlocked operating-system credential store.
On Linux, sign in to a desktop session with D-Bus and a Secret Service provider, such as GNOME Keyring or KWallet.
An installation cannot unlock your credential store or create a provider account.

The download command needs an internet connection and trusted CA certificates.
Linux and macOS also need `curl`, `tar`, and a SHA-256 utility.
Windows needs 64-bit PowerShell 5.1 or later.

## Install a release

On Linux or macOS, run:

```sh
curl -fsSL https://github.com/Corvidae-Coding-Projects/Project-Peritus/releases/latest/download/install.sh | sh
```

On Windows, run:

```powershell
irm https://github.com/Corvidae-Coding-Projects/Project-Peritus/releases/latest/download/install.ps1 | iex
```

Run the command as your normal user. Do not run the complete installer with `sudo`.
The installer requests administrator access only for system dependencies.
Open a new terminal after installation. Then run `peritus`.

Each release installer downloads its own release version.
It checks the archive's SHA-256 value before extraction.
The native installer checks package files before it installs them.
These checks detect changed downloads. They do not replace trust in the release publisher.

The package includes the command, background process, terminal interface, and sandbox helper.
The installer also saves an uninstaller and configures the user command path.
It does not register an always-on system service.

## Runtime dependencies

The installer keeps working dependencies that are already present.
If a required dependency is missing, it uses the operating system's package tools.

| System | Dependency installation |
| --- | --- |
| Debian or Ubuntu | APT installs Git, bubblewrap, D-Bus, GNOME Keyring, and CA certificates. |
| Fedora | DNF installs the same runtime dependencies. |
| Arch Linux | Pacman installs the same runtime dependencies. |
| openSUSE | Zypper installs the same runtime dependencies. |
| macOS | Homebrew installs Git when Homebrew is present. Otherwise, Apple's Command Line Tools installer supplies Git. |
| Windows | WinGet installs Git and the Visual C++ runtime when required. |

On macOS, approve the Command Line Tools window if it appears. The installer waits up to 20 minutes for completion.
Windows needs App Installer from Microsoft Store if WinGet is not present and a dependency is missing.
Unsupported Linux distributions need the listed runtime dependencies before installation.

System package changes remain in place if Peritus installation later fails.
Removal of Peritus does not remove shared dependencies.

To prevent automatic system package changes, set `PERITUS_INSTALL_DEPS=0`.
The installer then stops if a required dependency is missing. This option does not disable dependency checks.

Build tools for your repositories are separate from Peritus runtime dependencies.
Install the languages, compilers, and test tools that your projects need.
Setup offers to install a missing Codex or Claude tool when you select that account provider.
It asks for confirmation before it runs the provider's official installer.
The provider owns that tool and its dependencies. Sign-in occurs after installation.
Provider accounts and local model weights remain separate.

## Select a release version

Download the installer from the required release tag. For example:

```sh
curl -fsSL https://github.com/Corvidae-Coding-Projects/Project-Peritus/releases/download/v1.2.3/install.sh | sh
```

Replace `v1.2.3` with a published version.
You can also set `PERITUS_VERSION` to a complete `vMAJOR.MINOR.PATCH` tag.
Pre-release tags are not supported by the current update protocol.

`PERITUS_REPOSITORY` selects another GitHub repository.
`PERITUS_RELEASE_BASE_URL` selects an archive source for controlled mirrors and local tests.
Only use overrides for a source that you trust. An override changes where executable code comes from.

## Install from a downloaded package

1. Open [GitHub Releases](https://github.com/Corvidae-Coding-Projects/Project-Peritus/releases).
2. Download the archive for your system and processor.
3. Download its `.sha256` file.
4. Compare the archive's SHA-256 value with that file.
5. Extract the archive.
6. Run its installer with the absolute package path.

On Linux or macOS:

```sh
sh /absolute/path/to/package/Install-Peritus.sh /absolute/path/to/package
```

On Windows:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File C:\absolute\package\Install-Peritus.ps1 -BundleRoot C:\absolute\package
```

Use the corresponding `Upgrade-Peritus` script to replace an existing installation.
It restores the previous package if installation fails.

## Update

For an archive/source installation, run `peritus update` to install an available update.
Startup checks occur at most once every six hours.
Use `peritus update --disable-checks` to stop automatic checks.
Use `peritus update --enable-checks` to start them again.

Debian and RPM installations use the system package manager instead, as described below.

## Debian and RPM packages

Releases also provide signed Debian 13 and Fedora 44 packages for x86-64 and ARM64.
These distribution packages have their own distribution-native library requirements;
the glibc baseline above applies to the portable Linux archive.

Download `peritus-deb-<architecture>.tar.gz` or `peritus-rpm-<architecture>.tar.gz`,
its `.asc` signature, and `peritus-release.asc` from the same release. Before
importing the public key, compare its full fingerprint with the reviewed value:

```text
C5FA A006 1C56 096C 0DB5 7E93 BC11 52DD CE40 9792
```

For example, to verify and install the x86-64 Debian bundle:

```sh
gpg --show-keys --with-fingerprint peritus-release.asc
gpg --import peritus-release.asc
gpg --verify peritus-deb-x86_64.tar.gz.asc peritus-deb-x86_64.tar.gz
tar -xzf peritus-deb-x86_64.tar.gz
cd packages
gpg --verify SHA256SUMS.asc SHA256SUMS
sha256sum --check SHA256SUMS
sudo apt install ./peritus_*-1_amd64.deb
```

Stop if the fingerprint, signature, or any checksum does not match. Use `arm64`
in the Debian filename and `aarch64` in the bundle name on ARM64. APT does not
automatically verify a local `.deb`'s embedded origin signature; the explicit
verification above is required.

For Fedora, verify and extract the matching RPM bundle in the same way, then:

```sh
sudo rpm --import peritus-release.asc
rpmkeys --checksig --verbose peritus-[0-9]*-1.fc44.x86_64.rpm
sudo dnf --setopt=localpkg_gpgcheck=1 install ./peritus-[0-9]*-1.fc44.x86_64.rpm
```

Use `aarch64` instead of `x86_64` on ARM64. Install the main `peritus` package;
debug packages and the source RPM are optional developer artifacts.
Run Peritus as your normal user, not as root. The public command is
`/usr/bin/peritus`; all four sibling binaries are under `/usr/lib/peritus`.
No system service starts during package installation.

To upgrade, verify the newer release bundle and install its package with APT or
DNF. `peritus update` and startup download prompts are disabled for these builds.
There is no hosted APT/DNF repository yet, so ordinary repository refreshes do
not discover Peritus releases. Remove with `sudo apt remove peritus` or
`sudo dnf remove peritus`; user configuration, credentials, and task state remain.
Avoid mixing a user-local archive installation with a system package, because
an existing user-local command may precede `/usr/bin/peritus` on `PATH`.

See [distribution build and signing instructions](distro/README.md) for source
packages, signature policy, and release-operator setup.

## Uninstall

Cancel active tasks before removal. Close the Peritus interface.

On Linux:

```sh
sh "$HOME/.local/share/peritus/Uninstall-Peritus.sh"
```

On macOS:

```sh
sh "$HOME/Library/Application Support/Peritus/share/peritus/Uninstall-Peritus.sh"
```

On Windows:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File "$env:LOCALAPPDATA\Programs\Peritus\share\Uninstall-Peritus.ps1"
```

Removal keeps configuration, credentials, task state, logs, and managed repository copies.
It also keeps system dependencies and the standard Unix user-bin path setting.
For an older installation without a saved uninstaller, use the uninstaller from its downloaded package.

## Install from source

Install the Rust toolchain specified in `rust-toolchain.toml`.
From the repository root, run:

```sh
cargo xtask product-install
```

This command builds a release package and runs the same native installer.
Rust and native build-tool installation are required only for source builds, not downloaded Peritus packages.

## Package and release checks

`cargo xtask product-package` creates `dist/peritus-<platform>-<architecture>`.
The package contains `manifest.toml`, `SHA256SUMS`, four executables, lifecycle scripts, and an inactive service template.

`cargo xtask release-bootstrap-smoke` tests installation, upgrade, removal, public download, and checksum rejection.
Native CI runs the lifecycle and all 18 H2 scenarios on each release target.

Tagged release jobs build each binary on its target system.
Windows x86-64 uses native `clang-cl` 20.1.8 for C dependencies and the MSVC ABI,
with the locked Rust toolchain and `/Brepro` final linking. This avoids an
observed MSVC C-code generation difference between otherwise identical SQLite
builds. The compiler version, target and executable hash are recorded in build
logs; unexpected compiler versions fail rather than falling back. CI also
compares two fresh bundled SQLite compilations. Windows ARM64 retains its
native toolchain. All six targets still require byte-identical independent
release archives; no executable bytes are rewritten to make comparisons pass.
They attach archives, checksums, inventories, SBOMs, provenance, and GitHub attestations to a draft release.
Publication requires all six target archives and their evidence, plus signed
Debian/RPM bundles and main packages for both Linux architectures.
The publisher then attaches version-bound `install.sh` and `install.ps1` files, with checksums, before it publishes the release.

These mechanisms do not approve a production release.
See [Release qualification](../docs/h4-release-qualification.md) for the approval requirements.
See [Native platform qualification](../docs/h2-platform-qualification.md) for the test protocol and retained evidence.
