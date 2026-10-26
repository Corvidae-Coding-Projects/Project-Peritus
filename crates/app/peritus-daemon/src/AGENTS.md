# crates/app/peritus-daemon/src — navigation

## Responsibility

Daemon entry/composition, local transport, authenticated command handlers, domain-service wiring, workspace admission, and product-run registry.

## Files in this folder

| File | Role |
| --- | --- |
| [api.rs](api.rs) | Explicit public facade; implementation modules retain their existing ownership. |
| [artifact.rs](artifact.rs) | Authority-owned upload/download transfer services. |
| [authority.rs](authority.rs) | Serialized authority actor and bounded client. |
| [cli.rs](cli.rs) | Process-facing command-line composition for `peritusd`. |
| [command.rs](command.rs) | Durable application-command admission and exact retained response construction. |
| [config.rs](config.rs) | Strict version-one daemon configuration. |
| [diagnostic.rs](diagnostic.rs) | Best-effort diagnostics that must never turn recovery into a startup failure. |
| [domain.rs](domain.rs) | Closed application-command dispatch into existing authoritative domain reducers. |
| [error.rs](error.rs) | Stable daemon failures and recovery instructions. |
| [identity.rs](identity.rs) | Stable non-secret daemon instance identity. |
| [instance.rs](instance.rs) | Exclusive process instance ownership. |
| [ipc.rs](ipc.rs) | Authenticated local IPC and bounded A3 frame transport. |
| [lib.rs](lib.rs) | Production daemon and application composition owner for Peritus. |
| [lifecycle.rs](lifecycle.rs) | Monotonic daemon startup and readiness state. |
| [outbox.rs](outbox.rs) | Durable C0 outbox delivery supervision. |
| [product_control.rs](product_control.rs) | Daemon-owned authoritative C0 product-control journal. Legacy run JSON is not a writer here. |
| [product_run.rs](product_run.rs) | Daemon-owned product-run registry, persistence, and execution admission. |
| [prompt.rs](prompt.rs) | Bounded ownership and authority admission for interactive prompts. |
| [qualification.rs](qualification.rs) | Production qualification seams shared by component-specific fault routes. |
| [rsi.rs](rsi.rs) | Native candidate evaluation and evidence-backed production evolution. |
| [session.rs](session.rs) | Authenticated A3 session establishment and connection ownership. |
| [shutdown.rs](shutdown.rs) | Pure, bounded coordination state for truthful daemon shutdown observations. |
| [startup.rs](startup.rs) | Deterministic production startup composition. |
| [subscription.rs](subscription.rs) | Canonical topic compilation and bounded live event delivery. |
| [telemetry.rs](telemetry.rs) | Redaction-safe bounded local telemetry export. |
| [terminal.rs](terminal.rs) | Live C2 process control projected through bounded A3 terminal attachments. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `ReadinessModel`, `StartupPhaseModel`, `mutation_admitted_model_exec`, `diagnostic_admitted_model_exec`, `startup_transition_model_exec`. |
| [worker.rs](worker.rs) | Structured bounded ownership for scheduler-dispatched Tokio worker tasks. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [artifact/](artifact/AGENTS.md) | Implementation pieces for `artifact.rs`: Authority-owned upload/download transfer services. |
| [authority/](authority/AGENTS.md) | Implementation pieces for `authority.rs`: Serialized authority actor and bounded client. |
| [bin/](bin/AGENTS.md) | Executable entrypoints for `peritus-daemon`; follow each binary into its package implementation. |
| [cli/](cli/AGENTS.md) | Implementation pieces for `cli.rs`: Process-facing command-line composition for `peritusd`. |
| [command/](command/AGENTS.md) | Implementation pieces for `command.rs`: Durable application-command admission and exact retained response construction. |
| [component/](component/AGENTS.md) | Explicit application component inventories. |
| [config/](config/AGENTS.md) | Implementation pieces for `config.rs`: Strict version-one daemon configuration. |
| [domain/](domain/AGENTS.md) | Implementation pieces for `domain.rs`: Closed application-command dispatch into existing authoritative domain reducers. |
| [instance/](instance/AGENTS.md) | Implementation pieces for `instance.rs`: Exclusive process instance ownership. |
| [ipc/](ipc/AGENTS.md) | Implementation pieces for `ipc.rs`: Authenticated local IPC and bounded A3 frame transport. |
| [outbox/](outbox/AGENTS.md) | Implementation pieces for `outbox.rs`: Durable C0 outbox delivery supervision. |
| [product_control/](product_control/AGENTS.md) | Implementation pieces for `product_control.rs`: Daemon-owned authoritative C0 product-control journal. Legacy run JSON is not a writer here. |
| [product_run/](product_run/AGENTS.md) | ProductRunService and the durable human workbench: request admission, interaction, persistence, recovery, progress, candidate actions, and improvement inbox execution. |
| [prompt/](prompt/AGENTS.md) | Implementation pieces for `prompt.rs`: Bounded ownership and authority admission for interactive prompts. |
| [qualification/](qualification/AGENTS.md) | Implementation pieces for `qualification.rs`: Production qualification seams shared by component-specific fault routes. |
| [rsi/](rsi/AGENTS.md) | Implementation pieces for `rsi.rs`: Native candidate evaluation and evidence-backed production evolution. |
| [session/](session/AGENTS.md) | Implementation pieces for `session.rs`: Authenticated A3 session establishment and connection ownership. |
| [shutdown/](shutdown/AGENTS.md) | Implementation pieces for `shutdown.rs`: Pure, bounded coordination state for truthful daemon shutdown observations. |
| [startup/](startup/AGENTS.md) | Implementation pieces for `startup.rs`: Deterministic production startup composition. |
| [subscription/](subscription/AGENTS.md) | Implementation pieces for `subscription.rs`: Canonical topic compilation and bounded live event delivery. |
| [telemetry/](telemetry/AGENTS.md) | Implementation pieces for `telemetry.rs`: Redaction-safe bounded local telemetry export. |
| [terminal/](terminal/AGENTS.md) | Implementation pieces for `terminal.rs`: Live C2 process control projected through bounded A3 terminal attachments. |
| [worker/](worker/AGENTS.md) | Implementation pieces for `worker.rs`: Structured bounded ownership for scheduler-dispatched Tokio worker tasks. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
