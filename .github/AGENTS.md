# .github — navigation

## Responsibility

GitHub workflow/action definitions. Configuration files describe workflows; live runner/enablement state is external to the repository.

## Files in this folder

| File | Role |
| --- | --- |
| [actionlint.yaml](actionlint.yaml) | YAML workflow/action declaration for actionlint. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [actions/](actions/AGENTS.md) | Repository-owned reusable action definitions used by hosted workflows. |
| [workflows/](workflows/AGENTS.md) | Hosted workflow declarations, including native release packaging. |

## Working in this area

Trace each workflow or composite action from its event trigger through permissions, inputs, called scripts, and produced artifacts. Repository YAML describes intended automation only; runner availability, secrets, branch protection, and current enablement remain live GitHub state.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
