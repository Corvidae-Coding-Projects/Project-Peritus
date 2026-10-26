# security — navigation

## Responsibility

Security policy/assessment assets and retained dependency findings; distinguish documents from runtime security enforcement.

## Files in this folder

| File | Role |
| --- | --- |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |
| [control-catalog-v1.toml](control-catalog-v1.toml) | Configuration/contract data for control catalog v1. Sections: `schema`, `owner`, `status`, `requirements`, `acceptance_criteria`. |
| [tcb-inventory-v1.toml](tcb-inventory-v1.toml) | Configuration/contract data for tcb inventory v1. Sections: `schema`, `owner`, `status`, `component`, `qualification`. |
| [threat-model-v1.toml](threat-model-v1.toml) | Configuration/contract data for threat model v1. Sections: `schema`, `owner`, `status`, `asset`, `threat`. |
| [unsafe-inventory-v1.toml](unsafe-inventory-v1.toml) | Configuration/contract data for unsafe inventory v1. Sections: `schema`, `owner`, `status`, `policy`, `category`. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [schemas/](schemas/AGENTS.md) | Versioned security-control, assessment, and campaign-report JSON contracts. |

## Working in this area

Use [`README.md`](README.md) to choose the local entry point, then follow the parent waypoint to understand callers, ownership, and neighboring systems. Inspect the referenced file and current call sites before changing behavior, and update this map whenever files or responsibilities move.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
