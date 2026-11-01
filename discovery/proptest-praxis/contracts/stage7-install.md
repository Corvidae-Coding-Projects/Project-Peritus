# Stage 7 Linux installation contract

The bootstrap caller is `install.sh`: it validates the release version, downloads an archive
and checksum over permitted protocols, checks archive shape/types, invokes the native installer
or upgrader depending on the installed `peritus` path, then reads back the exact version. The
native owners are `packaging/linux/{Install,Upgrade,Uninstall}-Peritus.sh`; `xtask` delegates
package installation to those scripts. The launcher later consumes the installed binaries and
user state. These are separate boundaries, so a static package tree is not an installed product.

`owner/stage7_bootstrap.py` constructs local `file://` releases and invokes the real bootstrap
under `/bin/sh` with a disposable HOME. It checks a valid archive by reading back the installed
command; invalid version, checksum, path, symlink and missing-asset cases must fail before
publishing package paths. User state and sibling files are independent canaries. The generated
cases vary fault classes and invalid version strings, retaining each exact input and response.
The checksum check is calibrated by `owner/calibrate_stage7_bootstrap.py`: the valid-source
checksum-mismatch replay passes, while a temporary copy with the digest comparison skipped
fails the same oracle. The production `install.sh` bytes are verified unchanged afterward.

`owner/stage7_install.py` invokes the real Linux lifecycle scripts with disposable package and
home trees. External system dependencies and browser setup are harmless local shims, while the
shell scripts, SHA256SUMS validation, file publication, backups, retry and uninstall paths are
real. The independent model records six package-owned paths before and after each operation,
plus sibling binaries, share data, settings, state, logs, credentials and profile contents. An
injected `install -m` failure at publication 1 through 6 must return nonzero without reporting
success. A failed upgrade must restore the exact prior package-file state, including a partial
state reached by a first failed install. A retry must publish all package files without changing
user data or duplicating the profile path line. Uninstall must remove only package-owned paths.

The initial script violated rollback on a reachable partial install. The minimized case is in
`cases/stage7-linux-upgrade-rollback.json`; the focused real-shell replay is
`packaging/replay_linux_upgrade_rollback.py`. It failed on the original Upgrade script and
passes with the candidate repair on the isolated fix branch. Generated fault schedules also
passed against the candidate before it was copied into discovery. The repair removes only
package-owned files published by a failed upgrade before copying the captured old files back;
it skips rollback cleanup if the backup itself was incomplete. User files are not deleted.

The broad lifecycle harness does not prove power-loss atomicity, preservation after arbitrary
signals during a file copy, or behavior of real package-manager dependency installation.
Linux success uses synthetic binaries and a stubbed browser setup, so actual release-binary
startup remains a distinct native package gate. macOS launchd and Windows PowerShell lifecycle
behavior require their respective native runners. Launcher onboarding, credential stores and
provider login are separate Stage 7 targets or explicit gaps.
