# webui/src — navigation

## Responsibility

Browser UI entrypoint, application composition, and client-side modules.

## Files in this folder

| File | Role |
| --- | --- |
| [App.svelte](App.svelte) | Browser application composition: projects/sessions, conversation transcript/composer, commands, recovery controls, and workbench panels. |
| [app.css](app.css) | Keep conversation authority visible at narrow widths and available to assistive tech. |
| [main.ts](main.ts) | Browser mount entrypoint and global application setup. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [lib/](lib/AGENTS.md) | Browser workspace/session state, API bindings, commands, file drafts, operations, and reusable presentation components. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
