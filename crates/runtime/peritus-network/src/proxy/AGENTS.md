# crates/runtime/peritus-network/src/proxy — navigation

## Responsibility

Bounded loopback HTTP/CONNECT proxy owner.

## Files in this folder

| File | Role |
| --- | --- |
| [accept.rs](accept.rs) | Nonblocking bounded listener acceptance. |
| [connect.rs](connect.rs) | Exact address connect and bounded bidirectional relay. |
| [http.rs](http.rs) | Bounded HTTP/1 request and response head parsing. |
| [inherited.rs](inherited.rs) | Unix descriptor bridge for a listener bound inside a fresh target network namespace. |
| [mod.rs](mod.rs) | Bounded loopback HTTP/CONNECT proxy owner. |
| [owner.rs](owner.rs) | Listener lifetime, worker bounds, and complete joins. |
| [redirect_worker.rs](redirect_worker.rs) | Per-request redirect successor selection and bounded response draining. |
| [worker.rs](worker.rs) | One authenticated, plan-bound proxy connection. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
