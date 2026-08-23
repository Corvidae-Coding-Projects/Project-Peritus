# peritus-projection

Pure deterministic journal folds, versioned checkpoints, and durable shadow rebuilds for Project
Peritus.

## Contract

Replay accepts only an integrity-checked `peritus-journal` export. Pure folds build lifecycle,
budget, authority, journal, actual artifact-reference, and evidence catalogs without receiving any
I/O capability. The SQLite adapter owns prefixed projection tables in a caller-selected shared
database file and atomically activates a fully checked shadow generation.

Replay starts at genesis and checks contiguous global positions, aggregate sequence/predecessor
chains, stable aggregate revision binding, registered B3 family/schema pairs, typed frame decoding,
and projection invariants. The resulting checkpoint binds projection identity/version and schema,
the final journal position and head digest, and the deterministic payload digest. The artifact fold
uses the journal export's actual committed batch dependencies rather than reconstructing references
from event payloads.

## Startup and rebuild

`ProjectionStore::plan_startup` compares the active generation with an exact journal report and
returns either reuse or a reasoned genesis rebuild. `rebuild_from_genesis` prepares a complete
in-memory candidate; `install_shadow` then advances the active pointer under an explicit
generation compare-and-swap in one SQLite transaction. A failure leaves the old active generation
unchanged, and a repeated rebuild of one journal/schema binding must have identical checksums.

Projection data is not authoritative and is never an input to journal repair. The current crate
does not provide incremental catch-up, automatic startup orchestration, or old-generation garbage
collection; an embedding application must drive each configured projection.

See [C0 durable state](../../../docs/c0-durable-state.md) for operator ordering, failure recovery,
and exact validation commands.
