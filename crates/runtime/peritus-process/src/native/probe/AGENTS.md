# crates/runtime/peritus-process/src/native/probe — navigation

## Responsibility

Implementation pieces for `probe.rs`: Production operating-system probe for durable process recovery.

## Files in this folder

| File | Role |
| --- | --- |
| [linux.rs](linux.rs) | Linux `/proc` birth-token and process-session recovery probe. |
| [macos.rs](macos.rs) | MacOS `proc_pidinfo` birth-token and process-session recovery probe. |
| [windows.rs](windows.rs) | Windows process creation-time recovery probe. |

Owning module: [probe.rs](../probe.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
