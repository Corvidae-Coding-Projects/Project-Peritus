# OpenCode malformed-turn real replay

## Responsibility

| File | Role |
| --- | --- |
| [README.md](README.md) | Ordinary credentials/configuration prerequisites, real-service commands, independent expectations, and scope. |
| [replay.py](replay.py) | Creates an owned external consumer project, builds with two jobs, and records real inference results. |
| [consumer.rs](consumer.rs) | Uses the production registry/model protocol, performs actual bounded fixture reads, and checks fresh tokens and exact report values. |
| [capture.py](capture.py) | Transparent observer of the official CLI's unchanged stdout/stderr; never substitutes provider responses. |
| [observed-invalid-turn.jsonl](observed-invalid-turn.jsonl) | Actual completed vendor response whose malformed model JSON stopped the pre-fix consumer. |

Implementation owner: [OpenCode adapter](../../../../crates/model/peritus-provider-opencode/src/AGENTS.md).
Campaign: [issue 127 report](../../opencode-malformed-output-real-2026-10-15.md).
## Working in this area

Start with [`README.md`](README.md) to recover prerequisites, disposable-state rules, and the independent oracle before running anything. Keep generated binaries and mutable run state outside the repository, and treat a replay as evidence for the exact observed build and scenario rather than universal product coverage.

Navigation: [Parent](../AGENTS.md) · [Repository](../../../../AGENTS.md).
