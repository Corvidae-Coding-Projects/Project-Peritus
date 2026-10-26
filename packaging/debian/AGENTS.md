# packaging/debian — navigation

## Responsibility

Debian package metadata templates and maintainer lifecycle scripts.

## Files in this folder

| File | Role |
| --- | --- |
| [control.in](control.in) | Checked-in control asset/configuration (.in). |
| [peritus.docs](peritus.docs) | Checked-in peritus asset/configuration (.docs). |
| [peritus.lintian-overrides](peritus.lintian-overrides) | Checked-in peritus asset/configuration (.lintian-overrides). |
| [peritus.manpages](peritus.manpages) | Checked-in peritus asset/configuration (.manpages). |
| [rules](rules) | Checked-in rules asset/configuration. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [source/](source/AGENTS.md) | Debian source-package format declaration used by the package recipe. |

## Working in this area

Start with the platform manifest or entry script listed above and trace the corresponding `xtask` or installer caller before changing an artifact. Preserve platform-specific install, upgrade, uninstall, ownership, and complete-package behavior together; a recipe or retained report is not proof of a published release.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
