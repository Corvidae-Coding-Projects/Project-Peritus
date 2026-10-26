# crates/runtime/peritus-sandbox-linux/src/native — navigation

## Responsibility

Linux-only safe native enforcement adapters.

## Files in this folder

| File | Role |
| --- | --- |
| [helper.rs](helper.rs) | Version-one helper handshake, enforcement installation, and literal target exec. |
| [landlock_policy.rs](landlock_policy.rs) | Safe Landlock ABI probing and hard-requirement policy installation. |
| [mod.rs](mod.rs) | Linux-only safe native enforcement adapters. |
| [rlimit.rs](rlimit.rs) | Safe helper rlimit installation. |
| [seccomp_policy.rs](seccomp_policy.rs) | Reviewed fixed syscall-class seccomp compiler and installer. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [helper/](helper/AGENTS.md) | Implementation pieces for `helper.rs`: Version-one helper handshake, enforcement installation, and literal target exec. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
