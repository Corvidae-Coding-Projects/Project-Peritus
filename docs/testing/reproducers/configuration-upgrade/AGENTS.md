# Generated configuration upgrade replay

This folder operates the public launcher API and actual packaged terminal/daemon against
disposable application directories. It does not substitute providers or daemon responses.

| File | Responsibility |
| --- | --- |
| [consumer.rs](consumer.rs) | Public `ProductBootstrap` consumer for real provider selections, observed folders, generation publication, and fresh-process preparation. |
| [build.py](build.py) | Links that consumer against the exact libraries emitted by an actual CLI/daemon Cargo build. |
| [replay.py](replay.py) | Historical-format fixtures, original released-binary failure, generated upgrades, rejection, interruption, concurrency, and terminal restart scenarios. |
| [README.md](README.md) | Contract, execution, dependency, and coverage boundaries. |

The production owners are the [launcher configuration](../../../../crates/app/peritus-launcher/src/bootstrap/configuration.rs)
and its [compatibility map](../../../../crates/app/peritus-launcher/src/bootstrap/configuration/AGENTS.md).
