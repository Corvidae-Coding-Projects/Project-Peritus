# Peritus foundation guide

## Folder guide

Shared identities, bounded codecs, authority/policy contracts, budgets, and acceptance/release vocabulary used by higher layers.

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [README.md](README.md) | Human overview and linked directory map for this architectural area. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [peritus-plugin-sdk/](peritus-plugin-sdk/AGENTS.md) | Stable plugin wire contracts, manifests, and module declarations. |
| [peritus-budget/](peritus-budget/AGENTS.md) | hierarchical resource-budget accounting for Project Peritus. |
| [peritus-codec/](peritus-codec/AGENTS.md) | Canonical versioned framing and bounded binary decoding for Peritus. |
| [peritus-kernel/](peritus-kernel/AGENTS.md) | lifecycle state machines and authoritative event reduction for Peritus. |
| [peritus-policy/](peritus-policy/AGENTS.md) | policy, capability, and authority-time core for Project Peritus. |
| [peritus-protocol/](peritus-protocol/AGENTS.md) | Stable canonical domain protocol for Peritus. |
| [peritus-quality-policy/](peritus-quality-policy/AGENTS.md) | exact-revision acceptance evaluator for Project Peritus. |
| [peritus-release-policy/](peritus-release-policy/AGENTS.md) | fail-closed production release policy for Project Peritus. |
| [peritus-security-policy/](peritus-security-policy/AGENTS.md) | H0 security-readiness policy for exact Peritus release candidates. |
| [peritus-spec/](peritus-spec/AGENTS.md) | immutable acceptance contracts for Project Peritus. |
| [peritus-tcb/](peritus-tcb/AGENTS.md) | Audited trust-boundary policy anchor for Project Peritus. |
| [peritus-types/](peritus-types/AGENTS.md) | peritus-types package. |

