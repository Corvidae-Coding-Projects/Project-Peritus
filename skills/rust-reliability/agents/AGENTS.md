# skills/rust-reliability/agents — navigation

## Responsibility

Model-client metadata for presenting the canonical Rust reliability skill.

## Files in this folder

| File | Role |
| --- | --- |
| [openai.yaml](openai.yaml) | YAML configuration/metadata for openai. |

## Working in this area

Start with the child package waypoint; load referenced resources only when the skill routes the task to them. Bundling supplies guidance and resources but no execution authority, so keep tool permissions and runtime behavior in their host owners and preserve package-relative links.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
