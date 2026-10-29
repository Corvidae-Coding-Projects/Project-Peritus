# skills/rust-reliability/scripts — navigation

## Responsibility

Optional Rust build/lint evidence helper bundled with the skill; behavioral confidence comes from operating the actual program.

## Files in this folder

| File | Role |
| --- | --- |
| [rust_checks.py](rust_checks.py) | Run real Cargo build/Clippy checks; record evidence without claiming behavior. |

## Working in this area

Start with the child package waypoint; load referenced resources only when the skill routes the task to them. Bundling supplies guidance and resources but no execution authority, so keep tool permissions and runtime behavior in their host owners and preserve package-relative links.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
