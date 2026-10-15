# peritus-telemetry

## Folder guide

Bounded OpenTelemetry-compatible projections and exporters for Peritus.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-telemetry`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Bounded OpenTelemetry-compatible projections and exporters for Peritus. |

`peritus-telemetry` derives stable metrics and OpenTelemetry-compatible records from checked C7
trace projections. It owns deterministic bounded queues, explicit exporter acknowledgement,
shutdown flushing, durable V2 contiguous final-disposition checkpoints, and restart recovery. V2
requires exact `exported + dropped == submitted == disposed_through` accounting; V1 checkpoints are
unsupported. The family-60 observation protocol remains independently versioned at schema 1.

The crate sees only redaction-safe C7 values. It cannot run work or mutate Peritus authority.
See `docs/c7-trace-telemetry.md` for operational integration guidance.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
