# peritus-resilience

`peritus-resilience` is the runtime-neutral H1 release-qualification harness for an integrated
Peritus system. It defines a deterministic fault catalog, a black-box subject contract, bounded
observations, invariant evaluation, canonical evidence, and the only `Ready` verdict issued by
this crate.

The production catalog covers both sides of the journal, blob, snapshot, lease, patch, gate, and
promotion durable-commit boundary; hash-divergent authoritative and derived state; disk exhaustion;
provider, tool, and worker death; retry exhaustion; daemon death in every E0 active phase; and host
reboot during outstanding effects and reconciliation. Every scenario receives a fresh subject.

## Subject contract

An adapter implements `ResilienceSubject` and `ResilienceSubjectFactory`. The runner performs these
stages in order:

1. create a fresh isolated subject and prepare its active baseline;
2. arm and observe the exact catalog fault;
3. restart or reconcile and return direct state, ownership, retry, resource, and evidence facts;
4. consume the subject through bounded cleanup.

The contract is async without selecting a runtime. A runner-owned cancellation token is cancelled
if the runner future is dropped while setup, execution, recovery, or cleanup is pending. Subjects
must additionally own synchronous RAII teardown because async cleanup cannot run from `Drop`.

## Ready semantics

`Ready` is possible only for the built-in H1 production profile when all 43 cases ran against
fresh subjects, all private invariants passed, all cleanup completed, and the report retained a
canonical SHA-256 evidence digest. Custom catalogs are useful for diagnosis but always return
`NotReadyForProduction`.

The invariants reject, among other things, a fault that was not reached, an unexpected recovery,
new acceptance after disruption, divergent crash recovery, undetected corruption, mutation after
authoritative corruption, unaccounted or orphaned work, retry/resource overruns, incomplete
evidence anchors, noncanonical milestones, and incomplete cleanup. An execution error, panic, or
teardown error is infrastructure failure and never success.

This crate does not inject production faults itself. Adapters translate its typed scenarios into
the integrated daemon's failpoint, process, storage, provider, tool, and reboot controls. Production
H1 evidence therefore requires a real release-candidate adapter; an in-memory fake can test the
harness but cannot qualify a release.
