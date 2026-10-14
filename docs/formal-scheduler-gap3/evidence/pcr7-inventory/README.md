# Final candidate proof-impact inventory

## Folder guide

Historical evidence for pcr7 inventory; recorded inputs, outputs and source identities belong to the earlier investigation.

The original record below is preserved as historical evidence. Use current source and current product guides for present behavior.

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |
| [artifact-sha256.txt](artifact-sha256.txt) | Retained evidence/output for artifact sha256. |
| [base-candidate-name-status.tsv](base-candidate-name-status.tsv) | Retained evidence/output for base candidate name status. |
| [base-inventory.json](base-inventory.json) | Structured base inventory data with fields `hash_algorithm`, `limits`, `schema`, `schema_version`, `sources`. |
| [candidate-inventory.json](candidate-inventory.json) | Structured candidate inventory data with fields `hash_algorithm`, `limits`, `schema`, `schema_version`, `sources`. |
| [gate-commands.txt](gate-commands.txt) | Retained evidence/output for gate commands. |
| [gates.json](gates.json) | Structured gates data. |
| [packages.json](packages.json) | Structured packages data. |
| [packages.tsv](packages.tsv) | Retained evidence/output for packages. |
| [source-freeze.json](source-freeze.json) | Structured source freeze data with fields `base_commit`, `base_tree`, `candidate_commit`, `candidate_tree`, `merge_base`. |
| [summary.json](summary.json) | Structured summary data with fields `snapshot`, `base_source_count`, `candidate_source_count`, `transition_count`, `transition_kinds`. |
| [transitions.json](transitions.json) | Structured transitions data. |

This audit-only bundle records the exact proof-impact comparison from protected base
`a41113d1389c3d8fc77a00cc562cb9a5bcb7adc8` (tree
`2902edba7095a6ebd53b02bbf6f19005193d6deb`) to reviewed implementation candidate
`5d739de2902de0a5379f23f32b0c9f0ae562b28c` (tree
`3ca8f9e7fa5388ae3baa127c51c4bb5bd5c4fed7`).

The source-derived inventory contains 3,640 base sources, 3,731 candidate sources, and 155
transitions: 91 additions, 64 changes, and no removals. The affected projection contains all 66
packages because the architecture registry changed. Their classes are 53 H, 12 V, and one T.
The resulting plan contains 66 ordinary test commands and 66 Verus commands.

`artifact-sha256.txt` binds the generated artifacts and passes native `sha256sum --check`. The
bundle is evidence for the reviewed source and required verification plan. It is not a PCR
authorization, approval verdict, obligation discharge, protected-base trust record, or permission
to merge. Historical review and obligation records remain unchanged.

The gate list retains generator order, pairing each package's ordinary and Verus commands. A future
authorization verdict must apply the checker's canonical ordering by gate kind and then package.
That authorization must already exist on the protected base before a later application can pass
`verify-trust`; this draft pull request does not claim that protected-base state.
