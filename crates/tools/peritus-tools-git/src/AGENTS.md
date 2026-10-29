# crates/tools/peritus-tools-git/src — navigation

## Responsibility

Structured Git tools for immutable and authorized Peritus workspaces.

## Files in this folder

| File | Role |
| --- | --- |
| [catalog.rs](catalog.rs) | Canonical Git tool descriptors. |
| [decoder.rs](decoder.rs) | Strict schema-validated Git argument projection. |
| [dispatch_support.rs](dispatch_support.rs) | Shared terminal and caller-binding support for Git dispatchers. |
| [dispatcher.rs](dispatcher.rs) | Router-owned, cancellable Git execution with joined worker lifetimes. |
| [error.rs](error.rs) | Stable Git-tool failures and recovery guidance. |
| [input.rs](input.rs) | Checked bounded Git-tool inputs. |
| [lib.rs](lib.rs) | Structured Git tools for immutable and authorized Peritus workspaces. |
| [operation.rs](operation.rs) | Router-authorized Git dispatcher adapters. |
| [read.rs](read.rs) | Structured Git observations anchored to C1 immutable identities. |
| [render.rs](render.rs) | Bounded structured, model, and human Git renderings. |
| [schemas.rs](schemas.rs) | Exact version-one Git schemas. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `diff_bounds_valid`. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [dispatcher/](dispatcher/AGENTS.md) | Implementation pieces for `dispatcher.rs`: Router-owned, cancellable Git execution with joined worker lifetimes. |
| [render/](render/AGENTS.md) | Implementation pieces for `render.rs`: Bounded structured, model, and human Git renderings. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
