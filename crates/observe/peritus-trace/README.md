# peritus-trace

`peritus-trace` is C7's durable, causal, redaction-safe observation boundary. It encodes inert
family-60 trace observations, commits them through C0, rebuilds pure projections from checked
journal history, and exposes only closed diagnostics and encrypted artifact-vault references.

It deliberately has no execution, policy, approval, budget, provider transport, tool dispatch, or
gate-decision capability. See `docs/c7-trace-telemetry.md` for the integration and recovery contract.
