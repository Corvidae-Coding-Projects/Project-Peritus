# Installation

## Folder guide

Install/distribution assets: native package recipes, launch/service assets, notices, manual pages, and platform entrypoints.

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |
| [archive.py](archive.py) | Canonical native release archives using Python's standard library (3.12+). |
| [replay_archive_junctions.py](replay_archive_junctions.py) | Windows CLI replay for root, nested, and dangling directory junction rejection, plus ordinary archive readback. |
| [native-build.md](native-build.md) | Document: Native release compilation. |
| [native_build.py](native_build.py) | Compile native daemon libraries and their binary consumers in observed phases. |
| [native_inputs.py](native_inputs.py) | Observed inputs for native daemon libraries and their same-run binary consumers. |
| [native_release.py](native_release.py) | Build and package the current release on its native host. |
| [native_transport.py](native_transport.py) | Bounded, same-candidate transport of native daemon libraries, never a binary cache. |
| [real_startup.py](real_startup.py) | Install and operate an actual Unix release package on a disposable CI runner. |
| [rebuild.py](rebuild.py) | Retain real assembly observations and compare the complete native output inventory. |
| [windows_release.py](windows_release.py) | Native x86-64 Windows release builds with an explicit, reproducible C compiler. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [containers/](containers/AGENTS.md) | Pinned Debian/RPM package-builder container recipes and their dependency/toolchain setup. |
| [debian/](debian/AGENTS.md) | Debian package metadata templates and maintainer lifecycle scripts. |
| [distro/](distro/AGENTS.md) | Distribution package identities, release-input metadata, dependency selection, and preparation notes. |
| [keys/](keys/AGENTS.md) | Checked-in public release-signing key material; private signing credentials are external. |
| [licenses/](licenses/AGENTS.md) | Supplemental upstream license notices shipped with source bundles. Preserve the notices and their exact scope. |
| [linux/](linux/AGENTS.md) | Linux native install/upgrade/uninstall scripts, desktop entry, and user-service integration. |
| [macos/](macos/AGENTS.md) | macOS native install/upgrade/uninstall scripts and launch-agent integration. |
| [man/](man/AGENTS.md) | User-facing manual pages for Peritus native commands. |
| [rpm/](rpm/AGENTS.md) | RPM specification and package lifecycle/install layout declarations. |
| [schemas/](schemas/AGENTS.md) | Versioned native-package/platform report contracts retained for packaging and release consumers. |
| [windows/](windows/AGENTS.md) | PowerShell native install/upgrade/uninstall scripts and Windows launch/service integration. |

### Replay Windows archive junction rejection

On Windows with Python 3.12+ and Windows PowerShell, run from the repository root:

```powershell
python packaging/replay_archive_junctions.py
```

The replay creates disposable local fixture directories and junctions, invokes the
actual `packaging/archive.py` CLI, and removes its fixtures. It checks eight cases:
ordinary ZIP and tar.gz content readback, plus rejection of nested, root, and dangling
junctions in both formats. Rejection must happen before the destination is created.
Use `--work-dir PATH` to choose the fixture parent and `--archive-script PATH` to
replay against another revision's archive script.

At revision `0690a488118cc5b90006f65fe0a262f6be58ba4e`, all six junction cases
failed: nested and root junctions included an outside fixture's file in the archive;
dangling junctions failed only after creating a partial archive. The contract in
`archive.py` permits only regular directories/files and rejects links before writing.
The corrected command rejects all three junction shapes while retaining ordinary
archive behavior. This replay checks static fixture trees; concurrent filesystem
replacement during archiving is outside its scope.

Install the latest public release below, select a specific published version, or use
[Install from source](#install-from-source) for development builds.

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

### macOS 0.0.7 command-path error

If setup finishes but startup reports a missing `~/.local/bin/peritusd`, launch the
installed command directly:

```sh
"$HOME/Library/Application Support/Peritus/bin/peritus"
```

The macOS installer places `peritusd` beside this command and creates a separate
`~/.local/bin/peritus` shortcut. Release 0.0.7 can look beside that shortcut instead
of its target. Direct invocation keeps your existing configuration and credentials.
The corrected launcher resolves the command's real path before finding its daemon.

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

Release `0.0.5` changes the journal schema and saved conversation/run formats. Back up the state
root and read the [release migration notes](../docs/release-migration-recovery.md) before upgrading
from an earlier release. Older saved product-run formats are not automatically migrated; update
the daemon and its clients together.

For an archive/source installation, run `peritus update` to install an available update.
Startup checks occur at most once every six hours.
Use `peritus update --disable-checks` to stop automatic checks.
Use `peritus update --enable-checks` to start them again.

Debian and RPM installations use the system package manager instead, as described below.

## Debian and RPM packages

Release `0.0.5` provides signed Debian 13 and Fedora 44 packages for x86-64 and ARM64.
Release `0.0.7` provides the six native archives; distribution packages must be built from source.
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

On Windows, installation and removal stop processes belonging to the selected installation,
including a daemon started directly by the launcher. Other installations are left running.
Removal reports an error if a remaining file lock or access denial prevents deletion; release
that lock and retry. It does not report success or remove the user PATH entry while files remain.

## Install from source

Install the Rust toolchain specified in `rust-toolchain.toml`.
From the repository root, run:

```sh
cargo xtask product-install
```

This command builds a release package and runs the same native installer.
Rust and native build-tool installation are required only for source builds, not downloaded Peritus packages.

## Package and release builds

`cargo xtask product-package` creates `dist/peritus-<platform>-<architecture>`.
The package contains `manifest.toml`, `SHA256SUMS`, four executables, lifecycle scripts, and an inactive service template.

The manually dispatched release workflow builds Linux, macOS, and Windows packages on native
x86-64 and ARM64 runners. `packaging/native_release.py` builds the CLI, daemon, terminal interface,
and sandbox helper, assembles their package, reads back the native executable versions, and creates
an archive with checksums. Windows x86-64 uses pinned LLVM `clang-cl` 20.1.8 with the MSVC ABI;
Windows binaries use deterministic final linking.

Unix release runners install their actual package into the disposable runner's home and
operate the installed command through offline setup, a daemon-connected terminal session,
client exit, and clean daemon shutdown. macOS uses the installer's separate CLI symlink,
so this journey exercises discovery of the daemon beside the real executable.

The release operator creates the artifact inventory, SPDX SBOM, and provenance. GitHub signs the
archive and SBOM attestations; the operator authenticates these signed artifacts and uploads them
to the unpublished release draft. The publisher waits for all six native archives before attaching
version-bound `install.sh` and `install.ps1` bootstraps and publishing the release.

Only the native package job is enabled. Repository tests, formal proof tooling, and qualification
campaigns are not run. Development behavior is exercised through the real-testing skill; the
completed campaign is reused when the application source is unchanged.
