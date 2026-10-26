# packaging/linux — navigation

## Responsibility

Linux native install/upgrade/uninstall scripts, desktop entry, and user-service integration.

## Files in this folder

| File | Role |
| --- | --- |
| [Install-Peritus.sh](Install-Peritus.sh) | Installation/operation script for Install Peritus. |
| [Uninstall-Peritus.sh](Uninstall-Peritus.sh) | Installation/operation script for Uninstall Peritus. |
| [Upgrade-Peritus.sh](Upgrade-Peritus.sh) | Installation/operation script for Upgrade Peritus. |
| [package.toml.in](package.toml.in) | Checked-in package.toml asset/configuration (.in). |
| [peritus.service](peritus.service) | Checked-in peritus asset/configuration (.service). |

## Working in this area

Start with the platform manifest or entry script listed above and trace the corresponding `xtask` or installer caller before changing an artifact. Preserve platform-specific install, upgrade, uninstall, ownership, and complete-package behavior together; a recipe or retained report is not proof of a published release.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
