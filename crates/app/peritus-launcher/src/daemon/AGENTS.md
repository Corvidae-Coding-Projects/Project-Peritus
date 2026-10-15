# Shared-daemon launch coordination

This module lets additional terminal windows reuse one daemon. Workspace metadata is published as immutable generations; provider/runtime changes require a planned graceful restart.

| File | Role |
| --- | --- |
| [catalog.rs](catalog.rs) | Bounded cross-process launch lock, packaged binary/configuration comparison, catalog publication, real CLI status exchange, and generation acknowledgment. |

The [parent module](../daemon.rs) owns spawn/readiness supervision and executable identity. The [daemon catalog owner](../../../peritus-daemon/src/product_run/workspace_catalog.rs) admits workspace updates; [bootstrap configuration](../bootstrap/configuration/AGENTS.md) renders them. Configuration publication and acknowledgment do not execute a coding task.

Navigation: [Parent](../AGENTS.md) · [Package](../../AGENTS.md) · [Repository](../../../../../AGENTS.md).
