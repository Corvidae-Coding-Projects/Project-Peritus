# Plugin transport diagnostics

| File | Role |
| --- | --- |
| [diagnostics.rs](diagnostics.rs) | Owned stderr reader, bounded byte tail, exit status, and joined cleanup. |

The [transport](../transport.rs) attaches this evidence to failed exchanges. Stderr is drained
continuously so a noisy plugin cannot fill its pipe. Tail retention does not stop plugin work.
