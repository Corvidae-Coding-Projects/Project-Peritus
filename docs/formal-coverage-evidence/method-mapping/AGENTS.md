# docs/formal-coverage-evidence/method-mapping — navigation

## Responsibility

Historical evidence for method mapping; recorded inputs, outputs and source identities belong to the earlier investigation.

## Files in this folder

| File | Role |
| --- | --- |
| [app-runner.log.txt](app-runner.log.txt) | Retained evidence/output for app runner.log. |
| [artifact-sha256.json](artifact-sha256.json) | Structured artifact sha256 data with fields `app-runner.log.txt`, `baseline-regressions.log.txt`, `clippy.log.txt`, `commands.json`, `docs.log.txt`. |
| [baseline-regressions.log.txt](baseline-regressions.log.txt) | Recorded comparison baseline. |
| [clippy.log.txt](clippy.log.txt) | Retained evidence/output for clippy.log. |
| [commands.json](commands.json) | Structured commands data. |
| [docs.log.txt](docs.log.txt) | Retained evidence/output for docs.log. |
| [fmt.log.txt](fmt.log.txt) | Retained evidence/output for fmt.log. |
| [independent-review.md](independent-review.md) | Document: Independent review: proof-scope method ownership repair. |
| [peritus-product-runner-selected-functions.json](peritus-product-runner-selected-functions.json) | Structured peritus product runner selected functions data with fields `package`, `root_targets`, `registered_symbols`, `queried_functions`, `selected_functions`. |
| [peritus-product-runner-verus-output.txt](peritus-product-runner-verus-output.txt) | Retained evidence/output for peritus product runner verus output. |
| [protected-trust.log.txt](protected-trust.log.txt) | Retained evidence/output for protected trust.log. |
| [selected-functions.json](selected-functions.json) | Structured selected functions data with fields `peritus-product-runner`. |
| [source-identities.json](source-identities.json) | Structured source identities data with fields `base_commit`, `sources`. |
| [source-inputs.json](source-inputs.json) | Structured source inputs data with fields `git_commit`, `git_tree`, `protected_base`, `matches_committed_sources`, `source_sha256`. |
| [source-recheck.json](source-recheck.json) | Structured source recheck data with fields `captured_files`, `changed`, `matches_committed_sources`. |
| [tests.log.txt](tests.log.txt) | Retained evidence/output for tests.log. |

## Working in this area

Treat this directory as retained evidence tied to the source identity and environment recorded by its owning report. Do not infer current behavior from an old log alone; follow the parent waypoint to the active implementation or replay and preserve original evidence bytes when adding newer observations.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
