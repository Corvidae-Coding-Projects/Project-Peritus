# packaging/containers — navigation

## Responsibility

Pinned Debian/RPM package-builder container recipes and their dependency/toolchain setup.

## Files in this folder

| File | Role |
| --- | --- |
| [debian.Dockerfile](debian.Dockerfile) | Checked-in debian asset/configuration (.Dockerfile). |
| [fedora.Dockerfile](fedora.Dockerfile) | Checked-in fedora asset/configuration (.Dockerfile). |

## Working in this area

Start with the platform manifest or entry script listed above and trace the corresponding `xtask` or installer caller before changing an artifact. Preserve platform-specific install, upgrade, uninstall, ownership, and complete-package behavior together; a recipe or retained report is not proof of a published release.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
