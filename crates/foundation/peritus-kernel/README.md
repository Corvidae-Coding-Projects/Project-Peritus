# peritus-kernel

`peritus-kernel` is Peritus's effect-free lifecycle authority core. It owns the session, run,
attempt, turn, action, review, waiver, and acceptance state machines and emits logical event plans
for accepted commands.

The crate consumes exact-revision policy, budget, and acceptance facts from the B1/B2 verified
crates. It performs no persistence, I/O, clock reads, process execution, or durable authorization.
Commands are requests; only checked witnesses can advance authority-bearing phases.
