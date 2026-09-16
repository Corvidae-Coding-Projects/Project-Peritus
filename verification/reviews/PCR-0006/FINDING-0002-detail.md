# FINDING-0002 — command-restart fixture output framing

Original severity: medium, blocking.

Disposition: fixed.

The C2 candidate `08f2b7a3cfffdd612bb3033a39ef2d645e7e4f71` / `ef7b7ce79545e26b0ca68a7808b386e0f06452f7` stopped at canonical package gate 65. The nested command itself succeeded with exit code 0 and emitted `restart-effect:first`, but serial libtest left its `test ...` prefix open on the same line. The parent regression required an exact standalone marker line, so it failed even though the durable effect and process result were successful. C2's first 64 gate results and its later 67-command diagnostic continuation do not qualify this candidate and are not used as final gate evidence.

The exact reviewed candidate prefixes the fixture marker with one newline, which deterministically separates it from libtest's open prefix while preserving the exact-line assertion, exit-status checks, exactly-once `first`/`second` effect ledger, and fresh command-handle assertions. The old focused source fails and the repaired focused source, full `peritus-product-runner` suite, and strict Clippy pass in the retained evidence.

Source identity:

- `crates/app/peritus-product-runner/src/developer_tools/command_runtime/tests.rs`: C2 `94facc1ec45b9dc6af7a5dda1cf0f7bd5628d43822b372a61d56348fdd9e3be2`; candidate `161594f8bff3ddc48f64de1c7ab13e008ef82ee3b7be7956415b7d0c3e4c1889`
