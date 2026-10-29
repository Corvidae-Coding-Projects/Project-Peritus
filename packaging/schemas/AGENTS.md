# packaging/schemas — navigation

## Responsibility

Versioned native-package/platform report contracts retained for packaging and release consumers.

## Files in this folder

| File | Role |
| --- | --- |
| [h2-qualification-report-v1.schema.json](h2-qualification-report-v1.schema.json) | Peritus H2 qualification report v1 |
| [native-platform-cleanup-v1.schema.json](native-platform-cleanup-v1.schema.json) | Peritus native H2 cleanup response v1 |
| [native-platform-request-v1.schema.json](native-platform-request-v1.schema.json) | Peritus native H2 scenario request v1 |
| [native-platform-response-v1.schema.json](native-platform-response-v1.schema.json) | Peritus native H2 scenario response v1 |

## Working in this area

Start with the platform manifest or entry script listed above and trace the corresponding `xtask` or installer caller before changing an artifact. Preserve platform-specific install, upgrade, uninstall, ownership, and complete-package behavior together; a recipe or retained report is not proof of a published release.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
