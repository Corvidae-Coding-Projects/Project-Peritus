# crates/state/peritus-journal/src — navigation

## Responsibility

Transactional exact-byte event persistence for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [append_plan.rs](append_plan.rs) | Deterministic validation and hash-chain append planning. |
| [application.rs](application.rs) | Typed durable application ledger and catalog. |
| [authority.rs](authority.rs) | Durable authority-clock and credential-registry values. |
| [error.rs](error.rs) | Stable journal failures and recovery guidance. |
| [hash_chain.rs](hash_chain.rs) | Domain-separated event and batch hash construction. |
| [head.rs](head.rs) | Aggregate compare-and-append heads. |
| [idempotency.rs](idempotency.rs) | Deterministic command-idempotency decisions. |
| [identity.rs](identity.rs) | Validated journal-owned identities and aggregate kinds. |
| [integrity.rs](integrity.rs) | Integrity scan and exact export observations. |
| [lib.rs](lib.rs) | Transactional exact-byte event persistence for Peritus. |
| [outbox.rs](outbox.rs) | Transactional outbox inputs and durable delivery state. |
| [receipt.rs](receipt.rs) | Opaque move-only committed observations. |
| [record.rs](record.rs) | Exact frame records and checked state-install inputs. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `cas_successor`, `extends_sequence`, `committed_state_successor`, `registry_advance`, `authority_epoch_successor`. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [append_plan/](append_plan/AGENTS.md) | Implementation pieces for `append_plan.rs`: Deterministic validation and hash-chain append planning. |
| [application/](application/AGENTS.md) | Implementation pieces for `application.rs`: Typed durable application ledger and catalog. |
| [authority/](authority/AGENTS.md) | Implementation pieces for `authority.rs`: Durable authority-clock and credential-registry values. |
| [domain/](domain/AGENTS.md) | Typed B0/B1 durable transition adapters. |
| [integrity/](integrity/AGENTS.md) | Implementation pieces for `integrity.rs`: Integrity scan and exact export observations. |
| [record/](record/AGENTS.md) | Implementation pieces for `record.rs`: Exact frame records and checked state-install inputs. |
| [sqlite/](sqlite/AGENTS.md) | Narrow `SQLite` durability adapter. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
