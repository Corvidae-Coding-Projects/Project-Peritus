# crates/app/peritus-provider-onboarding/src — navigation

## Responsibility

Provider discovery, selection, credential references, official runtime installation/login, model discovery, and real connection stages.

## Files in this folder

| File | Role |
| --- | --- |
| [account.rs](account.rs) | Official executable discovery, status, and interactive login delegation. |
| [direct.rs](direct.rs) | Direct provider profiles and metadata discovery backed by existing operating-system credential references. |
| [error.rs](error.rs) | Redaction-safe onboarding failures. |
| [install.rs](install.rs) | Explicitly approved provider installation through the vendors' native installers. |
| [lib.rs](lib.rs) | Provider discovery and account authentication without credential custody. |
| [models.rs](models.rs) | Metadata-only provider discovery for synchronous setup, isolated from an ambient async runtime. |
| [status.rs](status.rs) | Minimal user-facing provider status facts. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
