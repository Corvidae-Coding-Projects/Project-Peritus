# peritus-approval

`peritus-approval` owns Peritus's pure, digest-bound human approval state machine. It validates
canonical request and credential inputs, authenticates Ed25519 decisions through one audited
verification surface, proves terminality and one-time logical consumption, and produces only
unprivileged logical transitions.

The crate does not sign decisions, deserialize external wire values, establish that a supplied
credential-registry snapshot is durably current, commit state, activate policy, or issue an effect
permit. Those responsibilities remain at their named integration boundaries.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-approval
```
