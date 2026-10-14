# Peritus rust reliability guide

## Folder guide

Canonical Rust reliability guidance, references, agent metadata, and an optional helper. Runtime activation/resource access lives in peritus-product-runner.

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [README.md](README.md) | Human overview and linked directory map for this architectural area. |
| [SKILL.md](SKILL.md) | Document: Rust Reliability. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [agents/](agents/AGENTS.md) | Model-client metadata for presenting the canonical Rust reliability skill. |
| [references/](references/AGENTS.md) | Rust guidance resources covering reliability, concurrency, ownership, errors, interfaces, and real behavioral scenarios. |
| [scripts/](scripts/AGENTS.md) | Optional Rust build/lint evidence helper bundled with the skill; behavioral confidence comes from operating the actual program. |

