# Peritus release operator

This binary is the narrow effect boundary for public release evidence. It reads one native package
record produced by `xtask`, builds deterministic inventory, SPDX 2.3, and SLSA provenance documents
with `peritus-release-artifacts`, and publishes those files only after the workflow supplies two
GitHub Sigstore attestation bundles.

The operator does not create signing keys or decide whether a candidate is ready. GitHub Actions
provides the short-lived signing identity, while the H4 qualification and release-policy crates
remain responsible for release admission.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-release-operator
```
