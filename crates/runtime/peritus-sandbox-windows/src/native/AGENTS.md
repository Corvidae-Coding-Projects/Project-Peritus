# crates/runtime/peritus-sandbox-windows/src/native — navigation

## Responsibility

Inventoried Windows FFI boundary for native probe, activation, and target creation.

## Files in this folder

| File | Role |
| --- | --- |
| [acl.rs](acl.rs) | Exact DACL backup ownership on retained filesystem object handles. |
| [handle.rs](handle.rs) | Protected-handle verification, exact launch whitelist, and `ConPTY` ownership. |
| [job.rs](job.rs) | Kill-on-close Job Object creation and hard resource limits. |
| [launch.rs](launch.rs) | Literal suspended target creation, Job Object assignment, and exact exit observation. |
| [mod.rs](mod.rs) | Inventoried Windows FFI boundary for native probe, activation, and target creation. |
| [path.rs](path.rs) | Windows volume identity query kept inside the inventoried FFI boundary. |
| [probe.rs](probe.rs) | Bounded native Windows capability probes. |
| [secret.rs](secret.rs) | Bounded protected-handle secret staging before target creation. |
| [token.rs](token.rs) | Restricted primary-token, low-integrity, and `AppContainer` identity setup. |
| [wfp.rs](wfp.rs) | Dynamic WFP session that permits only the exact AppContainer-to-proxy route. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [acl/](acl/AGENTS.md) | Implementation pieces for `acl.rs`: Exact DACL backup ownership on retained filesystem object handles. |
| [wfp/](wfp/AGENTS.md) | Implementation pieces for `wfp.rs`: Dynamic WFP session that permits only the exact AppContainer-to-proxy route. |

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
