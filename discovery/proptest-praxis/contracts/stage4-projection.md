# Stage 4 semantic catalog replay and shadow publication

Final bounded qualification is recorded in `../STAGE4-REPORT.md` and `../inventory/stage4-targets.json`; earlier worker-status notes describe their historical snapshot.

This public consumer drives production `JournalCatalogProjection`, `replay_from_genesis`, `rebuild_from_genesis` and `ProjectionStore` against an actual disposable C0 SQLite file shared with two real journal connections. Source is implemented; final-source build/lint/search/calibration evidence remains to be produced by the parent. No result is called passing before that execution.

## Read owners and actual callers

Read the projection package/source/sqlite maps; `lib.rs`, `fold.rs`, `journal_catalog.rs`, `encoding.rs`, `replay.rs`, `rebuild.rs`, `checkpoint.rs`, `catalog.rs`, `error.rs`, and `sqlite/{mod,store,swap,schema}.rs`, alongside the real journal export/input owners recorded in `stage4-journal.md`. Daemon `startup/projection.rs` performs actual integrity export, startup repair planning, rebuild and expected-generation installation for each built-in projection. It additionally validates changing-revision lineage owners before generic folds; this generic catalog campaign does not claim to replace those owner replays.

## Oracle independence

An ordinary append-event list, independent of all returned projection state, determines the complete semantic catalog: latest event of each aggregate, global position, local sequence, family, event/revision hash, complete event count and per-family multiplicities. The consumer constructs the frozen deterministic catalog payload from that semantic table and checks it byte-for-byte. Public counts, full payload, invariant checksum and checkpoint schema/position/head/payload bindings must all agree. Production encoding/fold/verified helpers never provide expected catalog values. Registered family/schema rejection is tested with genuine checked journal exports containing unsupported headers; the journal retains these canonical frame bytes without claiming their domain schema is supported.

The same event list independently determines journal-head identity through the journal target's explicit preimage contract. Using production built-in folds is essential: a harness-only custom fold is not used as the primary semantic implementation. A small `Projection` wrapper delegates normal catalog folding while selecting another schema or deliberately changing the encoded result, solely to test the public shadow store's schema/nondeterminism boundary.

## Histories and exact promises

Primary histories begin with two interleaved aggregates and distinct families, exposing global-position versus aggregate-sequence confusion. A separate generated/fixed `EmptyReplay` interaction operates a real empty journal, independently expects the complete empty catalog payload/head at position/count zero, installs generation one and reopens it to prove exact reuse. Generated histories append through either journal connection, save immutable export snapshots, replay, rebuild, publish under current/absent/older generation expectations, repeat identical rebuilds, publish older captured snapshots where the API permits, corrupt only an owned active payload, inject a real SQLite trigger failure after a shadow row is inserted but before pointer update, vary schema, close/reopen, and supply same-binding/different-checksum candidates.

`install_shadow` promises active-generation CAS and atomic publication; it does not promise that the candidate's captured journal point is still the newest journal head at installation. The model therefore permits an older internally consistent snapshot when generation CAS matches, then requires startup planning to report it stale against the current journal. No missing journal-freshness promise is invented.

Identical valid binding is reused without a new generation. A distinct binding or repair of corrupt payload allocates exactly the next durable generation. Stale CAS, same-binding nondeterminism and injected pointer-update failure must preserve the active pointer and retained generation count. The failure oracle requires the exact trigger's source diagnostic and `activate projection generation` operation, so an unrelated storage failure cannot count as the intended after-insert publication cut. Current active payload, checkpoint, invariant and actual stored bytes are checked after every operation and after real close/reopen. Reopen is not modeled by cloning an in-memory object. Fresh publication in the shared SQLite file invalidates a saved C0 replay observation; identical read-only reuse must not invent an external write.

Metrics record independent semantic folds, new/reused generations, stale-generation refusals, payload corruption/repair relationships, nondeterminism rejection, real failure after shadow insertion followed by continued use, schema-bound publication, real reopen and startup relationship decisions. Journal interaction counters are retained separately with a prefix. Maximum histories are 64 requested steps and four snapshot slots; frames/state are small.

## Calibration and gaps

`calibration/stage4-projection.patch` replaces the catalog's last global position with aggregate-local sequence. Both remain positive so the producer's basic invariant accepts the malformed catalog, while the independent semantic payload must reject it. Every initial fixture has global position two with local sequence one on its second aggregate; the fixed witness and broad capture/publication histories reach the mutated owner.

This target does not qualify all typed lifecycle/budget/authority/evidence/agent/artifact fold semantics, arbitrary corrupted export construction (the public integrity-export type is intentionally opaque), every schema migration, filesystem power loss, process kills inside an open transaction, every concurrency interleaving, backups or native OS enforcement. Its failure cut is a real SQLite statement error with transaction rollback, not a power-loss simulation. Separate targets cover the other semantic owners and durable state consumers.
