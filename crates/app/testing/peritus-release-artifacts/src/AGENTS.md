# crates/app/testing/peritus-release-artifacts/src — navigation

## Responsibility

Deterministic contracts for H4 release artifacts and supply-chain evidence.

## Files in this folder

| File | Role |
| --- | --- |
| [artifact.rs](artifact.rs) | Canonical release artifact inventory. |
| [binding.rs](binding.rs) | Exact candidate, version, toolchain, and platform bindings. |
| [documentation.rs](documentation.rs) | Migration, recovery, rollback, and license-document evidence. |
| [error.rs](error.rs) | Stable errors for artifact and evidence validation. |
| [identity.rs](identity.rs) | Validated identifiers and SHA-256 content identities. |
| [lib.rs](lib.rs) | Deterministic contracts for H4 release artifacts and supply-chain evidence. |
| [prelude.rs](prelude.rs) | Common H4 artifact imports for release assembly adapters. |
| [provenance.rs](provenance.rs) | Deterministic in-toto/SLSA-style provenance statements. |
| [reproducibility.rs](reproducibility.rs) | Reproducibility witnesses from independent builders. |
| [sbom.rs](sbom.rs) | Deterministic SPDX 2.3 SBOM generation from explicit component inputs. |
| [signature.rs](signature.rs) | Detached Ed25519 signature observations and verification. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
