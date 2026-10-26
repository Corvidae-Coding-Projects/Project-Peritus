# source-brief — navigation

## Responsibility

Source-backed brief workflow; the full Deep Research experience has its own design.

## Files

| File | Role |
| --- | --- |
| [module.toml](module.toml) | Four roles, their read-only tool allowlists and explicit output dependencies. |

Read callers for exact behavior. This map adds no testing or approval gates.
## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.
