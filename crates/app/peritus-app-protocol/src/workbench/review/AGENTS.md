# crates/app/peritus-app-protocol/src/workbench/review — navigation

## Responsibility

Implementation pieces for `review.rs`: Structured diff targets, anchored feedback, and qualification-evidence freshness.

## Files in this folder

| File | Role |
| --- | --- |
| [anchor.rs](anchor.rs) | Exact content- and lineage-bound structured-review anchors. |
| [diff.rs](diff.rs) | Bounded inert structured-diff files, hunks, and lines. |
| [diff_bytes.rs](diff_bytes.rs) | Exact digest-bound raw diff byte ranges. |
| [diff_page.rs](diff_page.rs) | Digest-bound structured-diff continuation over files, hunks, and line ranges. |
| [page.rs](page.rs) | Anchored review comments, evidence freshness, and bounded page projections. |
| [page_parser.rs](page_parser.rs) | Streaming structured diff pagination with bounded retained content. |
| [parser.rs](parser.rs) | Deterministic parsing of retained unified diffs into exact review targets. |
| [summary.rs](summary.rs) | Bounded digest-bound review metadata for peers using structured-diff pagination. |

Owning module: [review.rs](../review.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
