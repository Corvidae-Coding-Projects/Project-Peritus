# Plugin transport diagnostics

## Responsibility

| File | Role |
| --- | --- |
| [diagnostics.rs](diagnostics.rs) | Owned stderr reader, bounded byte tail, exit status, and joined cleanup. |

The [transport](../transport.rs) attaches this evidence to failed exchanges. Stderr is drained
continuously so a noisy plugin cannot fill its pipe. Tail retention does not stop plugin work.
## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.
