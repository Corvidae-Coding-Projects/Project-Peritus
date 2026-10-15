# OpenCode malformed-turn real replay

| File | Role |
| --- | --- |
| [README.md](README.md) | Ordinary credentials/configuration prerequisites, real-service commands, independent expectations, and scope. |
| [replay.py](replay.py) | Creates an owned external consumer project, builds with two jobs, and records real inference results. |
| [consumer.rs](consumer.rs) | Uses the production registry/model protocol, performs actual bounded fixture reads, and checks fresh tokens and exact report values. |
| [capture.py](capture.py) | Transparent observer of the official CLI's unchanged stdout/stderr; never substitutes provider responses. |
| [observed-invalid-turn.jsonl](observed-invalid-turn.jsonl) | Actual completed vendor response whose malformed model JSON stopped the pre-fix consumer. |

Implementation owner: [OpenCode adapter](../../../../crates/model/peritus-provider-opencode/src/AGENTS.md).
Campaign: [issue 127 report](../../opencode-malformed-output-real-2026-10-15.md).
Navigation: [Parent](../AGENTS.md) · [Repository](../../../../AGENTS.md).
