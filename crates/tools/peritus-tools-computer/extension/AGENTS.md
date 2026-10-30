# Chrome extension assets — navigation

## Responsibility

Private unpacked Chrome extension sources for controlling explicitly selected HTTP(S) loopback
development pages and reading explicitly selected external HTTPS tabs through Peritus's native-messaging bridge.

## Files in this folder

| File | Role |
| --- | --- |
| [manifest.json](manifest.json) | Fixed extension identity and narrowly scoped Chrome permissions. |
| [service-worker.js](service-worker.js) | Native-message dispatch, origin and selected-tab validation, tab ownership, observations, and page actions. |
| [popup.html](popup.html) | Local status, HTTPS tab sharing/revocation, and emergency pause. |
| [popup.js](popup.js) | Reads bridge status and shares/revokes the selected HTTPS tab, and pauses or resumes the local extension. |

The Rust installer embeds these exact files. Keep the extension private and unpacked; no store
publication or automatic Chrome policy installation belongs here.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Package map](../AGENTS.md) · [Repository map](../../../../AGENTS.md).
