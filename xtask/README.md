# Peritus xtask

## Folder guide

Repository packaging and release command owner. Read the current command dispatcher rather than old qualification instructions.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `xtask`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Packaging, installation, distribution, and release tools for Peritus. |

`xtask` owns repository packaging, installation, native compilation, and release staging.
It is separate from the installed CLI, the daemon, and the product's developer/reviewer/fixer workflow.
The root Cargo alias points to this package; command parsing lives in
[src/cli.rs](src/cli.rs), and the canonical help inventory is [src/cli/help.rs](src/cli/help.rs).

## Choose the owner

| Operation | Commands | Implementation |
| --- | --- | --- |
| Build a complete native package or install it for the current user | `product-package`, `product-install` | [Package lifecycle](src/product_package/AGENTS.md), [assembly wrapper](src/product_package.rs), [native package builder](../crates/app/testing/peritus-platform-qualification/AGENTS.md) |
| Create/stage a tag-bound GitHub release draft | `release-create`, `release-stage` | [Release owner](src/release/AGENTS.md), [release dispatch](src/release.rs) |
| Archive/checksum a native build or assemble downloaded same-run binaries | `release-package-stage`, `release-package-assemble` | [Release dispatch](src/release.rs), [archive writer](../packaging/archive.py), [native packaging](../packaging/native_release.py) |
| Build native daemon/CLI libraries and binaries | `release-daemon-library`, `release-daemon-binary`, `release-cli-library`, `release-cli-binary`, `release-windows-binary` | [Native compilation](../packaging/native_build.py), [Windows compilation](../packaging/windows_release.py), [compilation background](../packaging/native-build.md) |
| Retain native assembly observations | `release-rebuild-record` | [Rebuild operations](src/release/rebuild.rs), [observation recorder](../packaging/rebuild.py) |
| Build/retain package-builder images; compile, package, sign, inspect, and upload Debian/RPM artifacts | `distro-*` | [Distribution operations](src/distro.rs), [distribution assets](../packaging/distro/AGENTS.md) |

These commands are documented here, not executed by reading this guide. Installation changes the
current user's installation; signing and upload require their respective operator inputs. Release
staging leaves publication as a separate action. Inspect the dispatcher and the relevant script for
current environment variables and prerequisites.

## Related guides

- [Source installation and packaging](../packaging/README.md).
- [Release files and contracts](../release/AGENTS.md).
- [Native release workflow declaration](../.github/workflows/release.yml).
- [Repository navigation](../AGENTS.md).

Older documentation describes `all`, `docs-check`, architecture/source-layout checkers, formal
inventories, and CI qualification shards. Those external test/proof entrypoints have been retired;
they are absent from the current dispatcher. They do not describe the packaging commands above.
The product's own candidate checks and independent review remain separate application features.
