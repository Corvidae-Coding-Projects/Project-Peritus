# Shared-daemon launch coordination

## Responsibility

This module lets additional terminal windows reuse one daemon. Workspace metadata is published as immutable generations; provider/runtime changes require a planned graceful restart.

| File | Role |
| --- | --- |
| [catalog.rs](catalog.rs) | Bounded cross-process launch lock, packaged binary/configuration comparison, catalog publication, real CLI status exchange, and generation acknowledgment. |
| [local.rs](local.rs) | Read-only discovery from the live instance descriptor and waiting for local daemon ownership to end, without provider setup or launching another daemon. |

The [parent module](../daemon.rs) owns spawn/readiness supervision and executable identity. The [daemon catalog owner](../../../peritus-daemon/src/product_run/workspace_catalog.rs) admits workspace updates; [bootstrap configuration](../bootstrap/configuration/AGENTS.md) renders them. Configuration publication and acknowledgment do not execute a coding task.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent](../AGENTS.md) · [Package](../../AGENTS.md) · [Repository](../../../../../AGENTS.md).
