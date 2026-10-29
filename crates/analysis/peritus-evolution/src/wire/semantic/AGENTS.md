# crates/analysis/peritus-evolution/src/wire/semantic — navigation

## Responsibility

Strict production-limited semantic codecs shared by all six F0 frame families.

## Files in this folder

| File | Role |
| --- | --- |
| [attribution.rs](attribution.rs) | Canonical deterministic attribution records. |
| [binding.rs](binding.rs) | Canonical F0 limits, E1, E2, policy, review, and authority bridges. |
| [campaign.rs](campaign.rs) | Closed campaign command/event semantic payload codec. |
| [change.rs](change.rs) | Canonical change-manifest and isolated-variant semantics. |
| [evaluation.rs](evaluation.rs) | Restart-complete E3 evidence bridge codec. |
| [mod.rs](mod.rs) | Strict production-limited semantic codecs shared by all six F0 frame families. |
| [pointer.rs](pointer.rs) | Closed production-pointer command/event semantic payload codec. |
| [proposal.rs](proposal.rs) | Canonical promotion, rollback, and publication actions. |
| [selection.rs](selection.rs) | Canonical deny-wins assessments and selection decisions. |
| [state.rs](state.rs) | Complete canonical campaign and production-pointer checkpoint semantics. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [state/](state/AGENTS.md) | Implementation pieces for `state.rs`: Complete canonical campaign and production-pointer checkpoint semantics. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
