# Developer trace reader

## Responsibility

[reader.rs](reader.rs) reads stable frame headers and selected payloads from a captured file length. It does not run agents or modify traces. The parent module owns tags, frames, and errors.
## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.
