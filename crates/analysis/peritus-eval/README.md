# peritus-eval

`peritus-eval` is the durable E3 boundary for reproducible harness evaluation. It freezes exact
dataset, E1 harness, C5 provider, execution, resource, seed, retry, metric, and infrastructure
policies; expands them into a deterministic D3 work ledger; retains every rollout outcome; and
publishes replayable statistical evidence through C0.

Candidate-visible task inputs and sealed evaluator inputs are separate types and effect
directives. The crate is runtime-neutral: later application composition maps its checked inert
execution requests to C2/C3 and returns exact observations. E3 cannot mutate or promote a harness,
accept a run, waive a finding, or grant authority.

The complete frozen contract is in [the E3 design](../../../.design/e3-evaluation.md).

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-eval
```
