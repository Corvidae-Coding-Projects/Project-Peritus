# xtask/src — navigation

## Responsibility

Packaging, installation, distribution, and release tools for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [cli.rs](cli.rs) | Dispatch for the retained package, installation, distribution, and release tools. |
| [distro.rs](distro.rs) | Reviewed entry points for native Debian and RPM package release tooling. |
| [error.rs](error.rs) | Typed invocation, filesystem, metadata, and operation failures for packaging/release commands. |
| [lib.rs](lib.rs) | Packaging, installation, distribution, and release tools for Peritus. |
| [main.rs](main.rs) | Executable entrypoint delegating process arguments to the packaging/release CLI dispatcher. |
| [product_package.rs](product_package.rs) | Product package build and installation. |
| [release.rs](release.rs) | Tag-bound GitHub release staging behind direct reviewed Cargo commands. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [cli/](cli/AGENTS.md) | Implementation pieces for `cli.rs`: Dispatch for the retained package, installation, distribution, and release tools. |
| [product_package/](product_package/AGENTS.md) | Implementation pieces for `product_package.rs`: Product package build and installation. |
| [release/](release/AGENTS.md) | Implementation pieces for `release.rs`: Tag-bound GitHub release staging behind direct reviewed Cargo commands. |

## Working in this area

Use the responsibility and tables above to choose the local entry point, then follow the parent waypoint to understand callers, ownership, and neighboring systems. Inspect the referenced file and current call sites before changing behavior, and update this map whenever files or responsibilities move.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
