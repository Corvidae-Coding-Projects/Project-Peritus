# Read-loop recovery replayers

## Responsibility

These are ordinary consumers of the real Peritus libraries and official provider adapter. They do not implement a replacement workspace, model, or persistence system.

| File | Purpose |
| --- | --- |
| [consumer.rs](consumer.rs) | NDJSON driver for actual memory/workspace APIs, plus an explicitly selected live official-provider ProductRunner path. It includes shared provider/input helpers from `../checkpoint-cache/context-consumer.rs`. |
| [build.py](build.py) | Build current Cargo libraries and link the consumer using the exact emitted artifact cohort. |
| [replay.py](replay.py) | Fixture-derived regressions and generated ranges, rollover, origin, rejection, cycle, and provider-envelope checks. |
| [review_evidence.py](review_evidence.py) | Actual immutable developer-evidence paging, final provider-view delivery credit, role isolation, and cold recovery. |
| [live_replay.py](live_replay.py) | Authenticated official-provider conversation, design, edit, and review; historical temporary-file evidence and independent operation of the resulting CLI. |
| [independent_archive_policy.py](independent_archive_policy.py) | Independent public-API replay of private-source exclusions through original, wrapped, and chained archive handles and derived state. |
| [independent_historical_policy.py](independent_historical_policy.py) | Copies a stopped actual-provider writer context; verifies exact historical bytes and current Read/private admission while the original temporary file stays deleted. |
| [independent-README.md](independent-README.md) | Independent oracle, generated boundary campaign, original failure, and evidence limits. |
| [independent-original-archive-policy.json](independent-original-archive-policy.json) | Compact retained original archive-exclusion failure. |
| [independent-final-archive-policy.json](independent-final-archive-policy.json) | Final native artifact identity and repaired archive-policy outcomes. |
| `README.md` | Observable contracts, invocation, baseline failures, scope, and evidence locations. |

Independent oracle scripts and their README, when present, cover permission/private-source chains, cold reopening, normalized paths, and role separation. Keep fixtures disposable and preserve distinct failed actions, not large successful transcripts.
## Working in this area

Start with [`README.md`](README.md) to recover prerequisites, disposable-state rules, and the independent oracle before running anything. Keep generated binaries and mutable run state outside the repository, and treat a replay as evidence for the exact observed build and scenario rather than universal product coverage.
