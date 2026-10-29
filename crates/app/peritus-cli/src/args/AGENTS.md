# crates/app/peritus-cli/src/args — navigation

## Responsibility

Implementation pieces for `args.rs`: Parses command families, global options, open/update/shutdown arguments, and exact input values.

## Files in this folder

| File | Role |
| --- | --- |
| [artifact.rs](artifact.rs) | Artifact command-family parsing. |
| [improvements.rs](improvements.rs) | Explicit suggestion inbox commands; no implicit evaluation. |
| [product.rs](product.rs) | Product-run command parsing kept separate from the general CLI grammar. |
| [terminal.rs](terminal.rs) | Terminal command-family parsing. |
| [types.rs](types.rs) | Parsed CLI command and argument values. |

Owning module: [args.rs](../args.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
