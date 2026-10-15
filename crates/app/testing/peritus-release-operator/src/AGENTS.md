# crates/app/testing/peritus-release-operator/src — navigation

## Responsibility

Native release evidence and publication effect boundary. Entry names: `main`.

## Files in this folder

| File | Role |
| --- | --- |
| [app.rs](app.rs) | Top-level operation dispatch. |
| [args.rs](args.rs) | Closed command-line grammar for the release operator. |
| [cargo_graph.rs](cargo_graph.rs) | Locked Cargo graph projection into exact SPDX components. |
| [clock.rs](clock.rs) | UTC timestamp conversion without ambient locale state. |
| [error.rs](error.rs) | Typed release-operator errors. |
| [evidence.rs](evidence.rs) | Candidate-bound inventory, SBOM, and provenance generation. |
| [files.rs](files.rs) | Bounded filesystem helpers for retained release evidence. |
| [main.rs](main.rs) | Native release evidence and publication effect boundary. Entry names: `main`. |
| [package_record.rs](package_record.rs) | Versioned handoff from native packaging to evidence generation. |
| [publish.rs](publish.rs) | Retention and publication of externally signed release evidence. |
| [repository.rs](repository.rs) | Exact candidate and source-tree observations. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
