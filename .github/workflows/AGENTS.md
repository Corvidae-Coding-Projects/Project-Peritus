# .github/workflows — navigation

## Responsibility

Hosted workflow declarations, including native release packaging.

## Files in this folder

| File | Role |
| --- | --- |
| [release.yml](release.yml) | YAML workflow/action declaration for release. |
| [discovery.yml](discovery.yml) | Manually dispatched mutation, sanitizer fuzzing, and property baseline campaigns; separate from releases. |
| [mutation-sweep.yml](mutation-sweep.yml) | One-time branch-triggered 1,020-shard whole-workspace mutation sweep split across four GitHub-sized batches with exact aggregate candidate accounting. |
| [mutation-shard.yml](mutation-shard.yml) | Reusable 255-job mutation batch that preserves complete findings while failing only incomplete shard execution. |

## Working in this area

Trace each workflow or composite action from its event trigger through permissions, inputs, called scripts, and produced artifacts. Repository YAML describes intended automation only; runner availability, secrets, branch protection, and current enablement remain live GitHub state.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
