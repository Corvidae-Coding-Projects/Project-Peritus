# First-release preparation state

Snapshot: 2026-09-07 UTC. This is preparation evidence, not release approval.

| Item | State |
| --- | --- |
| Work branch | `feature/first-release-packaging` |
| Base | `main` at `f71ff3022dc5b8ca45c3ab0effac9209eee8498a` |
| Intended merge target | `main`; follow release issue #57 and Git history for review/commit status |
| Release version | `0.0.1`, selected by the release owner; previous local qualification used `0.0.0` |
| Package maintainer | `Laurelai <dollspacegay@gmail.com>`; previous local builds used a qualification-only identity |
| Signing identity | `Peritus Release Signing <dollspacegay@gmail.com>` |
| Public fingerprint | `C5FAA0061C56096C0DB57E93BC1152DDCE409792` |
| Key expiry | 2028-09-05 |
| Private key handling | Owner-authorized encrypted export and passphrase provisioned directly into GitHub environment secrets; isolated import/signing check passed; no export retained in the repository |
| CI signing environment | `release-signing`: tags matching `v*` only; required reviewer `dollspace-gay`; self-review explicitly authorized; key/passphrase secrets provisioned |
| Tag / release URL | Not created; publication remains pending H4 qualification |

## Version 0.0.1 preparation checks

After the owner selected `0.0.1`, all 82 workspace packages and their exact
internal dependencies were updated together. The lockfile changed only those 82
package versions; third-party dependency pins are unchanged. The architecture
guard now enforces the shared release version, and the separately reviewed
workspace policy remains pinned to `0.0.1` with version-drift rejection coverage.

| Check | Result | Retained local evidence |
| --- | --- | --- |
| xtask unit/integration tests | 303 unit + 3 integration passed | `target/release-001-xtask-tests.log` |
| Package tooling regressions | 15 passed | `target/release-001-distro-tests.log` |
| CLI and launcher tests, default and system-package | 44 default + 45 system-package passed | `target/release-001-launcher-cli-tests.log`, `target/release-001-system-package-tests.log` |
| Strict changed-crate Clippy, all targets/features | Passed | `target/release-001-clippy.log` |
| Workspace check, all targets/features | Passed | `target/release-001-workspace-check.log` |
| Repository policy, formatting, workflow syntax, whitespace | Passed | `target/release-001-policy.log`; `cargo fmt`, `actionlint`, `git diff --check` |

The daemon test requires local Unix-socket access. Its sandbox-denied first run
was rerun unchanged with that access and passed. No provider calls or benchmark
campaigns were started. Versioned native distribution packages still need to be
built from the final committed candidate; the results below are historical
packaging qualification, not `0.0.1` artifact evidence.

## Earlier local packaging qualification (0.0.0)

| Check | Result | Retained local evidence |
| --- | --- | --- |
| xtask unit/integration tests | 302 unit + 3 integration passed | `target/xtask-package-tests-final.log` |
| Package tooling regressions | 15 passed | `target/distro-tools-test-final.log` |
| Launcher update ownership | 3 default + 4 system-package tests passed | `target/launcher-*-package-tests.log` |
| Strict all-target/all-feature Clippy for changed Rust crates | Passed | `target/package-clippy-final.log` |
| Repository policy, formatting, workflow syntax, whitespace | Passed | `target/packaging-policy-final.log`; `cargo fmt`, `actionlint`, `git diff --check` |
| Fedora 44 x86-64 real RPM/SRPM/debug build | Passed | `target/rpm-package-build-release.log` |
| RPM native signatures, unsigned/tampered rejection, DNF install/integrity/removal | Passed | `target/rpm-sign-final.log`, `target/rpm-verify-final.log` |
| Debian 13 x86-64 real source/binary/debug build | Passed | `target/debian-package-build-release.log` |
| Debian signatures, source/binary Lintian, APT install/integrity/removal | Passed | `target/debian-sign-final.log`, `target/debian-verify-final.log` |

Container builds were executed locally with Podman using the checked-in Dockerfiles,
not Docker Engine. Both package formats compile offline inside their distribution
containers. Source inventories, builder IDs, and unsigned hashes are recorded in
`dist/packages/<format>/peritus-<format>-build.json`; signed package sets and bundles
are under `dist/signed/<format>`. Earlier qualification attempts are preserved in
`target/qualification-*`; failed and successful build workspaces remain under
`target/package-*`.

Main signed-package SHA-256 values for these qualification snapshots:

```text
edb0170da08625f196d80066af3e310e4b722f3b75bb123089421ea6da1bd8be  peritus_0.0.0-1_amd64.deb
039c1db58e08ea60c33090ac27ca64e7c0224d07e7aa7fca28917fb314b20c1c  peritus-0.0.0-1.fc44.x86_64.rpm
```

These artifacts are uncommitted `0.0.0` qualification snapshots, not a release
candidate for publication. Their per-build source inventories are authoritative;
documentation and verifier refinements continued after some source snapshots.
Rebuild from the approved versioned commit before publishing.

## Still required before publication

- Review/commit the preparation and obtain hosted native CI evidence. ARM64
  Debian/RPM builds and the Linux/macOS/Windows release matrix were not executed
  by this local packaging qualification. Job timing under the retained 10-minute
  ceiling is not yet proven on hosted runners.
- Exercise the protected signing workflow on the approved tagged candidate.
  Secret provisioning and the local key/passphrase check are not a hosted CI run.
- Complete the separate H4 approval process before using the owner's publication
  authorization. No benchmark campaign was started or restarted for packaging.

See the [operator guide](README.md) for commands and signature/trust details.
