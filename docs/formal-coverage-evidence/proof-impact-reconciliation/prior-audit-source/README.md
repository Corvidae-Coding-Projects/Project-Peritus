# Prior audit source archive

These are byte-exact source files for the audit snapshot recorded by the existing
`validation.json`, `reconciler-review.md`, `authorization-audit.md`, and
`evidence-audit.md`. Their recorded source hashes describe this archived snapshot, not the
extended current tooling. The `.txt` suffix prevents archived Rust sources from being treated as
live `.rs` inputs. `prior-root-SHA256SUMS.txt` preserves the former root `SHA256SUMS` verbatim.

The archive retains evidence; it does not extend the earlier audit, approval, or authorization.

| Original path | Archived path | Expected SHA-256 from `validation.json` |
|---|---|---|
| `docs/formal-coverage-evidence/proof-impact-reconciliation/reconcile.py` | `prior-audit-source/docs/formal-coverage-evidence/proof-impact-reconciliation/reconcile.py.txt` | `3f1a8f4105781475721cff2fdd4c0dad3150a5ed35b85c770f8e604495b84dc0` |
| `docs/formal-coverage-evidence/proof-impact-reconciliation/reconcile_tests.py` | `prior-audit-source/docs/formal-coverage-evidence/proof-impact-reconciliation/reconcile_tests.py.txt` | `032a7cc4dc9c8b43b5f3f36250f8b298b33dc6564ee00635dde627aa41edf230` |
| `xtask/src/cli.rs` | `prior-audit-source/xtask/src/cli.rs.txt` | `9b8379591a22cb51d61e51d6c534fcb746b254816e62862c84518ee97b0509f9` |
| `xtask/src/cli/help.rs` | `prior-audit-source/xtask/src/cli/help.rs.txt` | `a131001cf60f52e4a106cde537af6e73aecfd1b7eccfb674d64832a542aeb58a` |
| `xtask/src/trust.rs` | `prior-audit-source/xtask/src/trust.rs.txt` | `ef8eb1cd0704573fe4659d39d9d5f800993f188d2e1f5d9802d4fcd782726267` |
| `xtask/src/trust/manifest_impact.rs` | `prior-audit-source/xtask/src/trust/manifest_impact.rs.txt` | `461f12cb2cc1cfc9893813e3e1345d85812dd2c94a8156fc8c05b8e2d668a036` |
| `xtask/src/trust/manifest_impact/snapshot.rs` | `prior-audit-source/xtask/src/trust/manifest_impact/snapshot.rs.txt` | `84878f82e7cb8311dcb29aeb3121213c7e2745167237efcf561e54cc55ab219b` |
| `xtask/tests/cli.rs` | `prior-audit-source/xtask/tests/cli.rs.txt` | `9208b82787a382d5534e1df4ce3962e5c68cabffd726bd6bb84e44a8c3f80b3c` |
