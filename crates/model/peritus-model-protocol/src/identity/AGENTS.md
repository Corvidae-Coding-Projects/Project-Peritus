# crates/model/peritus-model-protocol/src/identity — navigation

## Responsibility

Implementation pieces for `identity.rs`: Checked public names and redacted provider identities.

## Files in this folder

| File | Role |
| --- | --- |
| [public.rs](public.rs) | Public provider, model, extension, tool, and output names. |
| [sensitive.rs](sensitive.rs) | Sensitive request and provider identities with explicit redacted diagnostics. |

Owning module: [identity.rs](../identity.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
