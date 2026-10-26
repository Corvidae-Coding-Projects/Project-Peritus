# packaging/windows — navigation

## Responsibility

PowerShell native install/upgrade/uninstall scripts and Windows launch/service integration.

## Files in this folder

| File | Role |
| --- | --- |
| [Install-Peritus.ps1](Install-Peritus.ps1) | Requires -Version 5.1 |
| [Peritus.Task.xml.in](Peritus.Task.xml.in) | Checked-in Peritus.Task.xml asset/configuration (.in). |
| [Uninstall-Peritus.ps1](Uninstall-Peritus.ps1) | Requires -Version 5.1 |
| [Upgrade-Peritus.ps1](Upgrade-Peritus.ps1) | Requires -Version 5.1 |
| [package.toml.in](package.toml.in) | Checked-in package.toml asset/configuration (.in). |

## Working in this area

Start with the platform manifest or entry script listed above and trace the corresponding `xtask` or installer caller before changing an artifact. Preserve platform-specific install, upgrade, uninstall, ownership, and complete-package behavior together; a recipe or retained report is not proof of a published release.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
