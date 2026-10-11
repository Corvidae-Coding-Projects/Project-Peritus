# Feature: B3 Domain Protocol and Canonical Codec

## Summary

B3 adds `peritus-codec` and `peritus-protocol`, the stable byte boundary between Peritus's
verified domain model and later persistence, process, provider, tool, telemetry, and client
layers. `peritus-codec` owns a small canonical binary vocabulary, bounded decoding, versioned
frames, and SHA-256 over exact encoded bytes. `peritus-protocol` owns version-one DTOs and checked
conversions for B0 lifecycle messages, the B1 policy/budget facts carried across boundaries, and
complete B2 acceptance contracts.

Decoded values are data, never authority. A DTO can request a B0 command or reconstruct a checked
B1/B2 value only through the owning domain constructor. B1 capability transitions, approvals,
leases, receipts, and other proof-carrying authority values have no raw decode path.

## User-visible behavior

- Every B0 command, event, subject, phase, envelope field, and stable error is represented by a
  versioned public DTO and a canonical byte sequence.
- B1 action intent, policy amendment, policy definition, and budget data have stable domain
  representations suitable for hashing, transport, and later persistence.
- A complete B2 acceptance contract round-trips through canonical bytes and is revalidated by
  `AcceptanceContract::new` on decode.
- Encoders produce exactly one byte representation for a value. Decoders reject noncanonical
  tags, malformed lengths, truncation, trailing bytes, invalid UTF-8, invalid domain values, and
  configured resource-limit violations.
- Checked-in schema and client metadata are generated deterministically and CI rejects drift.

## Requirements

1. `peritus-codec` defines one versioned frame with fixed magic, format version, message family,
   schema version, reserved flags, payload length, and payload.
2. Integers are fixed-width big-endian. Booleans and options use one-byte closed tags. Enums use
   explicit stable `u16` tags. Variable bytes and strings use `u32` lengths. Collections use
   `u32` counts and canonical element order supplied by the owning domain type.
3. Decode limits independently bound total frame bytes, payload bytes, collection items, string
   bytes, opaque bytes, and nesting depth. Lengths are validated before allocation or slicing.
4. A decoder consumes the whole declared payload. Unknown versions/families/tags, nonzero flags,
   invalid UTF-8, truncation, length overflow, and trailing data return stable typed errors.
5. SHA-256 helpers hash exact complete canonical frames and return `Sha256Digest`; they never hash
   a lossy projection or silently truncate.
6. `peritus-protocol` exposes private-field DTOs with constructors/accessors and explicit stable
   tags. Serde representation details do not define the canonical binary contract.
7. Wire-to-domain conversion calls checked constructors for identifiers, one-based numbers,
   capability names, resource quantities, policy structures, budgets, and acceptance contracts.
8. Decoding a command produces only a `KernelCommand` request plus its `CommandEnvelope`; decoding
   cannot produce `KernelTransition`, `CapabilityUseTransition`, approval, lease, permit, receipt,
   accepted state, or other authority-bearing output.
9. B0 coverage includes all 35 command kinds, all 37 event kinds, all subjects, eight phase
   families, every envelope field, every `KernelErrorKind`, entity detail, and authority-input
   detail.
10. Canonical action-intent bytes bind action ID, actor, role, environment, capability, resource,
    operation class, media type, and bounded opaque operation payload. The resulting digest is the
    exact digest carried by B0/B1 action requests.
11. Canonical policy-amendment bytes bind base policy, successor policy, replacement tier, and the
    complete replacement layer. The decoder computes the digest and supplies it to
    `PolicyAmendmentProposal::new`; no caller-supplied digest is trusted.
12. B1 DTOs cover policy definitions and their nested selectors/rules, action intents, budget
    amounts/limits/requests/snapshots/receipts, stable budget failures, and their exact revisions.
13. B2 DTOs preserve every acceptance-contract component, including document references,
    requirements, exclusions, assumptions, gate graph, review policy, evidence declarations,
    completion policy, approval policy, and waiver policy. Decode recomputes the canonical contract
    digest and rejects an advertised identity/digest mismatch.
14. The protocol exports deterministic schema metadata and generated client discriminants from one
    registry. `--check` generation fails when checked-in outputs differ.
15. Minimal, realistic, boundary, corrupt, adversarial, and prior-version fixtures form a checked
    compatibility corpus. Golden bytes and digests are immutable once version one is frozen.
16. B3 discharges `REF-B3-B0-LIFECYCLE-BYTES`, `REF-B3-B1-DIGEST-BYTES`, and
    `REF-B3-B2-CONTRACT-BYTES` with executable evidence and registers both crates in architecture,
    proof, reproducibility, trust, and CI inventories.

## Acceptance criteria

- Both crates build as foundation-layer verification-class `H` packages with no unsafe code.
- Canonical primitive and frame tests cover exact maximums, one-over limits, malformed tags,
  truncation at every byte boundary, overflow-shaped lengths, and trailing bytes.
- Every B0 command/event/error variant appears in both a round-trip matrix and the schema registry;
  compile-time or test assertions reject missing mappings.
- Action and amendment golden vectors prove that changing each bound field changes the digest.
- Complete acceptance contracts round-trip byte-for-byte, reconstruct an equal checked contract,
  and reject each corrupt identity/content case.
- Budget and policy realistic fixtures round-trip and malformed/noncanonical collections fail in
  the owning B1 constructor rather than becoming domain values.
- Generated schema/client files are reproducible and `--check` is part of local and hosted gates.
- Focused tests, strict Clippy/rustdoc, architecture/source/trust/reproducibility checks, Verus
  workspace verification, and Gate A pass.

## Current architecture

B0 owns the effect-free `KernelAggregate` reducer and freezes the lifecycle command/event surface.
B1 owns verified policy, budget, lease, capability, and approval semantics; digest values are exact
bytes but some canonical preimages are reserved for B3. B2 owns checked immutable acceptance
contracts and exact evidence evaluation. A2 owns compatibility fixture conventions but no protocol
schema. There is no production storage or released wire version, so B3 can freeze version one
without migration.

The three open architecture reservations require B3 to preserve B0 lifecycle fields, bind B1
action/amendment digests to complete canonical preimages, and preserve complete B2 contracts.
C0, C1, C2, C4-C7, D0-D3, and later clients consume this boundary.

## Proposed design

### Crate boundary

`peritus-codec` depends on `peritus-types` and the pinned `sha2` implementation. It knows no B0,
B1, or B2 types. Modules own limits, errors, primitive reader/writer operations, frame parsing,
traits, and hashing.

`peritus-protocol` depends on `peritus-codec`, `peritus-types`, `peritus-kernel`, `peritus-policy`,
`peritus-budget`, `peritus-spec`, and `peritus-quality-policy`. Its modules are grouped by shared
primitives, lifecycle, policy, budget, acceptance, action intent, envelopes, schema, and codegen.
No root module contains implementation logic.

### Canonical frame

Version one uses this 16-byte header:

```text
offset  size  field
0       4     ASCII magic "PRTS"
4       2     codec format version, big-endian
6       2     message family tag, big-endian
8       2     family schema version, big-endian
10      2     flags, currently zero
12      4     payload length, big-endian
16      N     payload
```

The format version changes only when primitive framing rules change. Each message family owns its
schema version. Unknown versions are rejected rather than guessed. New optional fields require a
new family schema version, not trailing bytes or implicit defaults.

### Conversion and authority

DTO decode has two stages: canonical bytes to an unprivileged DTO, then `TryFrom<Dto>` through
domain constructors. Domain conversion errors retain both the protocol location and the owning
domain's stable error category. DTOs for observations and requests remain plain data.

There is deliberately no `Decode` implementation for opaque transitions, approvals, capabilities,
leases, receipts, contract bindings, kernel aggregates, or accepted outcomes. Later replay rebuilds
authority by feeding decoded events/requests through the verified reducers and durable C0 rules.

### Schema and generated clients

A canonical registry lists family/tag names, versions, fields, and enum discriminants. The B3
codegen binary renders a deterministic JSON schema catalog and a dependency-free TypeScript
discriminant module. Checked-in outputs live under `protocol/` and are compared byte-for-byte by
`--check`; normal builds do not execute code generation.

## Data and compatibility

Version one is the first stored/public domain protocol. Exact bytes, numeric tags, field order,
limits, hashing domains, and error codes become compatibility-sensitive when B3 merges. Changes
afterward add a new schema version and retain decoders plus migration fixtures for supported older
versions. Unknown future versions fail with an explicit unsupported-version error.

Golden fixtures store `.bin` bytes, a readable manifest, and SHA-256 digests. Fixture generation is
deterministic and checked; test code does not overwrite fixtures.

## Failure handling

`CodecError` reports a stable kind, absolute byte offset, and optional limit. `ProtocolError`
distinguishes codec failure, wrong family, unknown tag, missing/invalid field, domain validation,
advertised digest mismatch, and schema drift. Errors never include opaque payload contents.

Encoding is transactional at the API boundary: callers receive either a complete frame or an
error. Decoding never returns a partial DTO. Code generation writes only explicitly named B3
artifacts and check mode performs no writes.

## Security considerations

All input is untrusted. Bounds are checked before allocation, arithmetic uses checked operations,
UTF-8 is validated, flags/tags are closed, and trailing bytes are rejected. The codec contains no
unsafe code, filesystem access, process execution, ambient clock, randomness, or secret handling.
Only the codegen binary reads/writes the two generated artifact paths.

Digests provide content identity, not authenticity or authority. Raw decoded data cannot create an
effect permit or accepted state. Cryptographic signatures and durable currentness remain owned by
B1/C0.

## Verification

Focused commands:

```text
cargo test --package peritus-codec --package peritus-protocol --all-targets --all-features --locked
cargo clippy --package peritus-codec --package peritus-protocol --all-targets --all-features --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --package peritus-codec --package peritus-protocol --all-features --no-deps --locked
cargo run --package peritus-protocol --bin peritus-protocol-codegen --locked -- --check
just check
just gate-a
```

## Rollout and rollback

B3 has no production data migration. It lands before C0 and freezes version one. Before a C0
consumer merges, rollback is removal of the two crates and registrations. After C0 persists B3
frames, rollback must retain the version-one decoder and use a forward migration rather than
rewriting history.

## Open questions

None block implementation. Version one uses a custom explicit binary contract because its exact
bytes, bounds, and failure modes are reviewable and stable. Self-describing JSON remains a generated
schema/debug projection, not the authority or hashing format.

## Out of scope

- Transactional append, journal hash chains, replay projections, migrations, and commit receipts
  (C0).
- Application/daemon request negotiation and streaming client sessions (A3/G0).
- Tool-specific, provider-specific, telemetry, and plugin message bodies (C4/C5/C7/G3); B3 supplies
  their common bounded action/envelope primitives.
- Cryptographic signatures, authenticated approvals, and durable authority reconstruction (B1/C0).
- Filesystem, process, network, model, or orchestration effects.
