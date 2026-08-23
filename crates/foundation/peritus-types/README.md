# peritus-types

`peritus-types` owns Peritus's time-independent primitive domain values. It provides nominal
nonzero identifiers, exact SHA-256 digest bytes, one-based counters, validated capability names,
an exact cross-subsystem revision tuple, and checked resource quantities.

## Invariants

- Every identifier contains exactly 16 bytes and at least one byte is nonzero.
- Revisions, event sequences, and generations start at one and never wrap.
- A capability name is at most 128 ASCII bytes and matches
  `[a-z][a-z0-9-]*(.[a-z][a-z0-9-]*)*`.
- Capability-name canonical ordering is proved over its exact validated ASCII bytes; dots remain
  ordinary bytes and carry no authority inheritance.
- A digest stores exactly 32 bytes; this crate does not compute or authenticate hashes.
- Resource quantities admit zero and report overflow or underflow instead of wrapping.
- A revision tuple binds the acceptance specification, harness, workspace generation/revision,
  immutable policy, and provider profile without introducing time or authority decisions.

Fields remain private and every ordinary-Rust boundary is checked. Verus type invariants and
postconditions establish the same constraints for verified callers without adding caller-visible
preconditions.

## Dependency policy

This verification-class `V` crate depends only on the workspace-pinned `vstd`. It deliberately
does not provide serialization, random ID generation, UUID conversion, hashing, time, I/O, or
authority decisions.
