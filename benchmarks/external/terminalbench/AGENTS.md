# benchmarks/external/terminalbench — navigation

## Responsibility

Harbor/Terminal-Bench bridge: native-agent invocation, deadline resolution, credential checkpointing, and process-family cleanup.

## Files in this folder

| File | Role |
| --- | --- |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |
| [credential_state.py](credential_state.py) | Safe host checkpointing for credential state rotated by official provider CLIs. |
| [deadline.py](deadline.py) | Resolve Harbor's retained agent deadline into a Peritus work horizon. |
| [peritus_agent.py](peritus_agent.py) | Harbor custom-agent boundary for the native Peritus product runner. |
| [pin.toml](pin.toml) | Configuration/contract data for pin. Sections: `schema_version`, `harbor_repository`, `harbor_revision`, `harbor_revision_date`, `harbor_version`. |
| [podman_compose_provider.py](podman_compose_provider.py) | Make podman-compose satisfy Harbor's Compose-V2 preflight probe. |
| [process_supervisor.py](process_supervisor.py) | Cancellation-safe process supervision for the Harbor adapter. |
| [registries.conf](registries.conf) | Checked-in registries asset/configuration (.conf). |

## Working in this area

Read the profile or workload definition together with the runner that consumes it and the baseline metadata that identifies the compared build. Benchmark artifacts measure the stated scenario only; keep them separate from product acceptance and refresh provenance whenever inputs or methodology change.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
