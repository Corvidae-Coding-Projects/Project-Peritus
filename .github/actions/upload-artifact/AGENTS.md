# .github/actions/upload-artifact — navigation

## Responsibility

Composite action uploading required release artifacts with bounded retries and unique attempt names.

## Files in this folder

| File | Role |
| --- | --- |
| [action.yml](action.yml) | YAML workflow/action declaration for action. |

## Working in this area

Trace each workflow or composite action from its event trigger through permissions, inputs, called scripts, and produced artifacts. Repository YAML describes intended automation only; runner availability, secrets, branch protection, and current enablement remain live GitHub state.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
