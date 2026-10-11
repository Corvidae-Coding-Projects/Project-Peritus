# General capability fixture matrix

These fixtures reproduce broad agent-harness failure classes without using an upstream benchmark
task name, hidden verifier rule, or expected benchmark artifact. Every family has a successful
case, an honestly incomplete case, and a failure case. Tests assert typed Peritus state or directly
observed process behavior; model prose is never the oracle.

| Family | Owning crate and fixture | Executable assertion surface |
| --- | --- | --- |
| Completion | `peritus-external-benchmarks/completion` | Native candidate checkpoint and settlement disposition |
| Resume | `peritus-external-benchmarks/resume` | Product phase observations and provider invocation count |
| Performance | `peritus-performance-qualification/performance` | H3 qualification and regression verdicts |
| Lifecycle | `peritus-resilience-qualification/lifecycle` | Real process signal, observed exit, reap, and lifecycle obligation |
| Directional schema | `peritus-external-benchmarks/directional-contract` | Request/response field identities and directions |
| Browser semantics | `peritus-external-benchmarks/browser` | Standards HTML tree construction and browser obligation |
| Provider | `peritus-external-benchmarks/provider` | Capability selection, terminal recovery, fallback, and circuit counts |
| Repository | `peritus-external-benchmarks/repository` | Git baseline, candidate paths, nested repository, and external drift |
| Prerequisites | `peritus-platform-qualification/prerequisites` | Real executable resolution and exit status |
| Terminal | `peritus-platform-qualification/terminal` | PTY input, resize, reader recovery, signal or cancellation, exit, and reap |
| Adapter | `peritus-external-benchmarks/adapter` | Admission errors, trace preparation, deadline settlement, and publication receipt |

All inputs are bounded: the repository fixture uses 256 files, lifecycle processes have immediate
readiness observations, terminal children are explicitly terminated or complete after one input,
and provider fixtures are local deterministic adapters. No fixture contacts a paid provider or
runs an external benchmark.

## Focused checks

From the repository root, run the complete focused matrix with:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked \
  --package peritus-performance-qualification \
  --package peritus-resilience-qualification \
  --package peritus-platform-qualification \
  --package peritus-external-benchmarks \
  --all-features
```
