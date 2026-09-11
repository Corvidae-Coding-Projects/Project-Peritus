# FINDING-0002 — checkpoint and rewind recovery semantics

Original severity: high, blocking.

Disposition: fixed.

The frozen implementation records automatic before-images before admitted direct and command-scoped mutations, binds them to the host run, and seals only exact owned postimages after successful effects. Manual checkpoint sealing remains intact. Rewind persists a Prepared operation and recovery checkpoint before C1, leaves uncertain C1 errors Prepared, and reconciles from exact action-marker, patch, manifest, and all-covered-path observations. Receipt lookup is observational; mutation and branch publication occur only on the authorized retry path. Settlement invalidates stale context without consuming bounded input capacity.

Conversation-only, files-only, and combined modes are distinct. Conversation-only performs no folder capture or mutation and needs no Write permission. Combined publication occurs only after file settlement, with retry-safe recovery. Conflicts preserve third-party bytes.

The retained recovery, mode, coverage, query side-effect, and full-ledger concerns are closed for this candidate.
