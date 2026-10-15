# crates/model/peritus-provider-openai/src/bin/codex_runtime_fake — navigation

## Responsibility

Implementation pieces for `codex_runtime_fake.rs`: Portable hermetic executable used only by this package's runtime conformance tests.

## Files in this folder

| File | Role |
| --- | --- |
| [behavior.rs](behavior.rs) | Test-only executable entry behavior. |
| [contract.rs](contract.rs) | Hardened invocation assertions for the fake executable. |
| [mod.rs](mod.rs) | Composition of the test-only executable concerns. |
| [scenario.rs](scenario.rs) | Deterministic current-JSONL scenarios selected through the configured model. |
| [trace.rs](trace.rs) | Per-installed-executable request trace. |

Owning module: [codex_runtime_fake.rs](../codex_runtime_fake.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
