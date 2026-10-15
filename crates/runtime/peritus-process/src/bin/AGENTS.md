# crates/runtime/peritus-process/src/bin — navigation

## Responsibility

Executable entrypoints for `peritus-process`; follow each binary into its package implementation.

## Files in this folder

| File | Role |
| --- | --- |
| [native_helper_fixture.rs](native_helper_fixture.rs) | Native helper protocol fixture implementation used by process integration tests. |
| [peritus-native-helper-fixture.rs](peritus-native-helper-fixture.rs) | Native-helper protocol fixture binary used by process integration tests. |
| [peritus-process-fixture.rs](peritus-process-fixture.rs) | Process-behavior fixture used by the crate's production adapter tests. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [peritus-process-fixture/](peritus-process-fixture/AGENTS.md) | Implementation pieces for `peritus-process-fixture.rs`: Process-behavior fixture used by the crate's production adapter tests. |

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
