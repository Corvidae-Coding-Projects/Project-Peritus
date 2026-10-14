# Peritus runtime guide

## Folder guide

Concrete host effects: process ownership, sandboxes, network access, secret delivery, filesystem patches, workspaces, and Git operations.

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [README.md](README.md) | Human overview and linked directory map for this architectural area. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [peritus-git/](peritus-git/AGENTS.md) | Structured Git repository and detached-worktree operations for Project Peritus. |
| [peritus-local-socket/](peritus-local-socket/AGENTS.md) | Bounded, owner-protected filesystem locations for local Peritus sockets. |
| [peritus-network/](peritus-network/AGENTS.md) | outbound connection planning and bounded managed proxy. |
| [peritus-patch/](peritus-patch/AGENTS.md) | typed patches and recoverable filesystem transactions for Peritus. |
| [peritus-process/](peritus-process/AGENTS.md) | Authorized owned process, PTY, output, cancellation, and recovery backplane. |
| [peritus-sandbox/](peritus-sandbox/AGENTS.md) | Checked platform-neutral sandbox contracts and executable reference semantics. |
| [peritus-sandbox-linux/](peritus-sandbox-linux/AGENTS.md) | Fail-closed Linux native sandbox preparation, helper, observation, and recovery. |
| [peritus-sandbox-macos/](peritus-sandbox-macos/AGENTS.md) | Deterministic macOS Seatbelt preparation and supervised native sandbox lifecycle. |
| [peritus-sandbox-windows/](peritus-sandbox-windows/AGENTS.md) | Fail-closed Windows native sandbox preparation, helper, observation, and recovery. |
| [peritus-secrets/](peritus-secrets/AGENTS.md) | Exact secret leases, zeroizing delivery, redaction, and credential-store adapters. |
| [peritus-workspace/](peritus-workspace/AGENTS.md) | Target-owned workspace authorization, snapshots, rollback, and restart reconciliation. |

