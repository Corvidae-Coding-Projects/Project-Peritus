# webui/public — navigation

## Responsibility

Static files copied into the browser build, including source-controlled notices and other public assets.

## Files in this folder

| File | Role |
| --- | --- |
| [favicon.svg](favicon.svg) | Checked-in favicon asset/configuration (.svg). |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [licenses/](licenses/AGENTS.md) | Frontend dependency license texts distributed with the static browser assets. |

## Working in this area

Start with the nearest component or state entry point, then trace UI state through the typed API boundary to the Rust gateway and daemon owner before changing behavior. Keep presentation-only state separate from durable run authority, and update the local component or file route when the browser surface moves.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
