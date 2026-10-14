# packaging/distro — navigation

## Responsibility

Distribution package identities, release-input metadata, dependency selection, and preparation notes.

## Files in this folder

| File | Role |
| --- | --- |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |
| [RELEASE-STATE.md](RELEASE-STATE.md) | Document: First-release preparation state. |
| [build.py](build.py) | Build genuine Debian source/binary packages or source/binary RPMs. |
| [ci_sign.py](ci_sign.py) | Explicit CI-only import into an ephemeral keyring; local signing never exports a key. |
| [common.py](common.py) | Shared, fail-closed helpers for distribution package tooling (Python 3.12+). |
| [compiled.py](compiled.py) | Transfer one native compilation into its same-run full package recipe. |
| [licenses.py](licenses.py) | Exact-version mapping for upstream notices omitted from published crate roots. |
| [main.py](main.py) | Audited entry point for first-party distribution release operations. |
| [sign.py](sign.py) | Sign packages via GnuPG's restricted agent socket; never export private keys. |
| [sign_inside.py](sign_inside.py) | Container-side native package signing and Debian checksum closure. |
| [source.py](source.py) | Create a source-complete, locked, offline-buildable upstream release tarball. |
| [upload.py](upload.py) | Upload only a verified, exact-commit distribution package set to an existing draft. |
| [verify.py](verify.py) | Run signature and installed-package qualification in disposable containers. |
| [verify_inside.py](verify_inside.py) | Cryptographic and real package-manager lifecycle checks; only runs in a container. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
