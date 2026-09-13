# FINDING-0003 — native Windows trust-manifest path reconciliation

Original severity: medium, blocking.

Disposition: fixed.

Both C2 Foundation Windows edge and Gate A Windows edge failed `materialized_candidate_general_trust_accepts_a_complete_candidate`. Trust occurrence reconciliation used `Path::to_str()` as a manifest key, yielding native `crates\foundation\...` spelling on Windows while the immutable manifest correctly uses repository `crates/foundation/...` spelling. The mismatch failed closed with one stale-entry diagnostic and one unmatched-occurrence diagnostic; it did not bypass a trust check.

The exact reviewed candidate centralizes canonical repository-path encoding: it accepts only a nonempty relative sequence of normal UTF-8 components and joins those components with `/`. Trust occurrence reconciliation now uses that encoding, while invalid or non-UTF8 paths emit a diagnostic and remain unmatched. Verdict-directory reconciliation reuses the same helper. The retained evidence includes both hosted red traces, the complete xtask suite, strict Clippy, and fresh green results for both Windows custody edges.

Checker source identities:

- `xtask/src/trust/manifest_file.rs`: C2 `4d65e806d86a08168edee7b22d64b25ded2ddc136218b9d5a4dcbab3b13f539a`; candidate `d2219fd374029e0b7476d494a156ac4ba10ffadcccedd60c6da896c26fc66afb`
- `xtask/src/trust/manifest_trust.rs`: C2 `884717f81d40bea7c2bf489936dee6c1e240184e817f35d576cd3638ec3fc81e`; candidate `12b9de9a323e4bae3672f1b4af3d714ec3fd1b866d49599b8bfc2182edf95603`
- `xtask/src/trust/manifest_impact/verdict/directory.rs`: C2 `ac66efa582e9fe353b1a094e7f965ee626d50050b0d3fb366105fee7d61495d2`; candidate `df7f7d3dfd36e9a41302a8f3e6d14c1bc4942bfac6150c365a434169cd48c086`
