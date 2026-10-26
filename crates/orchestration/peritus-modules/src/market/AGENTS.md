# market — navigation

## Responsibility

Additive module-market snapshot formats. Legacy executable discovery stays independently readable;
current clients bind source metadata to the same immutable catalog commit.

| File | Role |
| --- | --- |
| [source.rs](source.rs) | Source entry validation and atomic schema-two catalog-set persistence. |

[Parent map](../AGENTS.md). Refresh is inert: source discovery never compiles or installs code.
## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.
