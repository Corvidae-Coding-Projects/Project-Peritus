# B3 protocol refinement discharge

B3 closes the three canonical-byte reservations previously owned by B0, B1, and B2. The
reservations are no longer listed as open in `architecture.toml`; this record preserves the exact
discharge evidence and authority boundary.

## REF-B3-B0-LIFECYCLE-BYTES

`KernelCommandDto`, `CommandEnvelopeDto`, `KernelEventDto`, `KernelErrorDto`,
`KernelSubjectDto`, and `LifecyclePhaseDto` preserve every public B0 variant and field. The event
and error DTOs intentionally have no conversion into kernel-produced authority. The exhaustive
matrices in `crates/foundation/peritus-protocol/tests/lifecycle.rs` cover all 35 commands, 37 event
kinds, eight subject kinds, and 44 phases, plus exact revision, predecessor, sequence, malformed
tag, and invalid-identifier behavior.

## REF-B3-B1-DIGEST-BYTES

`ActionIntentDto` hashes the complete family-20 frame: action, actor, role, environment, resource,
capability name, operation class, media type, and opaque payload. Its only B0 proposal helper uses
that exact digest. `PolicyAmendmentDto` hashes a separate family-22 content frame containing the
base policy, successor policy, declared tier, and complete replacement layer; the family-23
proposal decoder rejects any advertised digest mismatch before checked B1 construction. Policy
and digest tests live in `crates/foundation/peritus-protocol/tests/policy.rs`.

## REF-B3-B2-CONTRACT-BYTES

`AcceptanceContractDto` hashes a family-30 content frame containing the acceptance ID, all eight
document references, requirements, exclusions, assumptions, complete gate definitions, review
policy, evidence declarations, completion policy, approval policy, and waiver policy. The
family-31 decoder verifies the advertised digest and reconstructs through every B2 checked
constructor. Tests in `crates/foundation/peritus-protocol/tests/acceptance.rs` prove round-trip,
checked reconstruction, and mismatch rejection.

## Compatibility evidence

`peritus-protocol-codegen` deterministically emits the family registry schema, TypeScript client
declarations, six representative golden frames, and their SHA-256 inventory. `--check` rejects
stale artifacts. `crates/foundation/peritus-protocol/tests/compatibility.rs` independently
regenerates, compares, and decodes the checked-in corpus. Decoding establishes syntax and domain
validity only; it never proves provenance, durable commit, evidence freshness, or effect authority.

The registry also reserves the merged production orchestration families without changing the
version-one frame format: D3 scheduler command/event/state use 70-72, D3 collaboration uses 73-75,
E0 orchestrator uses 76-78, and E1 harness command/event/state uses 79-81. Their owning crates define the complete canonical payload codecs;
the B3 registry and generated TypeScript/schema declarations provide stable global family identity.
