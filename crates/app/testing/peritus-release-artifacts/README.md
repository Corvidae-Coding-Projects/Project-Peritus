# peritus-release-artifacts

`peritus-release-artifacts` is the H4 effect-boundary contract crate for release inputs. It
validates an exact candidate binding, inventories immutable artifacts, renders deterministic SPDX
2.3 and SLSA-style provenance documents, verifies detached Ed25519 signatures, compares outputs
from independent builders, and inventories migration/recovery/license documentation.

The crate is deliberately incapable of generating keys, signing, tagging, publishing, or turning
an observation into a release decision. Callers supply explicit bytes and identities; successful
construction means only that the evidence is internally consistent and content-addressed.

Canonical JSON is compact UTF-8 emitted from fixed-field structs and path/key-sorted vectors. No
ambient time, host, Git state, environment variable, or network lookup participates in generation.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-release-artifacts
```
