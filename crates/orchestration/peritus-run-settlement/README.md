# peritus-run-settlement

`peritus-run-settlement` is the pure V-class domain for recording the latest candidate observed
during a coding run and deriving one honest terminal disposition. It separates candidate delivery
from automated acceptance: a candidate may be available for inspection or continuation without
claiming that its gates, public obligations, and independent review all passed.

The crate performs no filesystem, provider, process, clock, persistence, protocol, or user-interface
effects. Effectful callers supply checked observations. The reducer validates candidate lineage,
monotonic checkpoint order, evidence provenance, stage consistency, and exactly-once settlement.

`CandidateCheckpoint::observe` owns evidence reconciliation and bounds requested work progress by
the evidence that currently applies. `reobserve` uses those same rules for restart and handoff.
Observation sequences advance; qualification may decrease when a newer check fails or evidence is
revoked. An unchanged workspace must remain accessible in that case, without retaining a historical
success claim. Snapshot bindings currently cover source and Git history together; callers must not
interpret this digest as a content-only identity or assume it covers an unobserved toolchain.

Acceptance is fail closed. `RunDisposition::Accepted` is produced only for a qualified checkpoint
whose gate, obligation, and review evidence are current and satisfied. Provider, context, gate,
review, repository, adapter, deadline, cancellation, and recovery causes remain distinct.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-run-settlement
cargo verus verify --package peritus-run-settlement --locked --check-toolchain \
  --fwd-verus-args-to roots -- --no-cheating --rlimit 20
```
