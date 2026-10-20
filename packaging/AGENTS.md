# packaging — navigation

## Responsibility

Install/distribution assets: native package recipes, launch/service assets, notices, manual pages, and platform entrypoints.

## Files in this folder

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

## Child areas

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

## Nearby owners

- [Unix bootstrap installer](../install.sh).
- [Windows bootstrap installer](../install.ps1).
- [Packaging command implementation](../xtask/AGENTS.md).
- [Release assets](../release/AGENTS.md).

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
