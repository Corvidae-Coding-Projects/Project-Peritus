# peritus-agent

`peritus-agent` is the D0 durable inner coding loop. Its pure half validates immutable turn,
role, revision, provider, and limit bindings; reduces one causally fenced command to one event and
successor state; and reconstructs exactly the same state by replaying canonical events.

Its ordinary runtime modules compose that reducer with existing Peritus boundaries without taking
over their authority:

- C6 role-scoped context selection, evidence-only memory retrieval, and C5 message rendering;
- B1 held, active, settled, cancelled, and indeterminate budget reservations around model and tool
  effects, including usage high-water observations and exact terminal reconciliation;
- C5 pull-based provider streams with a commit-before-acknowledge envelope boundary;
- C4 inert tool preparation, independent exposure/authorization, bounded dispatch and control;
- C0/B3 atomic command, event, and checkpoint persistence plus checked restart replay; and
- a cooperative `AgentDriver` that performs one bounded transition or effect step at a time.

The active loop is context → model → proposed tools → authorization → execution → result recording
→ context, with a separate completion-proposal path. Pause, resume, cancellation, provider retry,
failure, exhaustion, and crash recovery are explicit reducer transitions. A dispatched effect with
no recoverable terminal observation becomes indeterminate and is never silently redispatched.
Durable provider envelopes rebuild the C5 reducer prefix before an exact continuation resumes.

The production-facing `DeveloperLoop` composes the existing C5 session boundary with an explicit
application tool executor and durable trace port. It serializes model tool calls, records each
provider envelope before reduction, records each tool observation before returning it to model
context, and continues until bounded final text is produced. Product adapters supply the concrete
workspace tools; D0 itself still grants no filesystem or process authority.

Model output is never tool authority, and D0 completion is never run acceptance. B0/B1/C0/C4 own
the receipts that authorize effects; later D1/D2/E0 components own gates, review, orchestration,
and acceptance.

Focused qualification:

```text
CARGO_BUILD_JOBS=2 cargo test -p peritus-agent --all-targets --all-features --locked
CARGO_BUILD_JOBS=2 cargo clippy -p peritus-agent --all-targets --all-features --locked -- -D warnings
CARGO_BUILD_JOBS=2 RUSTDOCFLAGS='-D warnings' cargo doc -p peritus-agent --all-features --no-deps --locked
```

See [`docs/d0-agent-loop.md`](../../../docs/d0-agent-loop.md) for lifecycle, recovery, operating,
and verification details.
