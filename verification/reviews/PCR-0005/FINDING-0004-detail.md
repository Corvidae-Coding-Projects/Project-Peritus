# FINDING-0004 — historical fork state and isolated/read-only authority

Original severity: high, blocking.

Disposition: fixed.

The frozen fork path loads the checkpoint's exact historical revision and carries a bounded seed containing immutable input history, brief, context, prompt view, images, files, and replies. It excludes execution, goal qualification, reviews, checkpoints, restores, credentials, approvals, leases, and permissions. Inherited queued inputs become Held, and historical file references become non-refreshable snapshots.

Writable forks require a separately registered non-overlapping workspace and exact equality for every covered checkpoint path before the child and parent reservation are atomically published. Governed allocations are reserved from the parent cumulative ceiling. Prepared combined rewinds durably reserve their child identity across restart until exact publication. Read-only branches lose Write and Process and admit only the bounded chat/governance execution shapes; governed children must use the exact allocated goal budget.

The retained historical-state, materialization, child-collision, pending-action, read-only-route, and budget-reset concerns are closed for this candidate.
