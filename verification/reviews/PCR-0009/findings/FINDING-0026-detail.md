# FINDING-0026: Repair integration violated required layout, lint, and test gates

Severity: low

Blocking: true

Disposition: fixed in the final reviewed candidate.

Several intermediate repair states exceeded the source-layout limit, split modules with inaccessible child helpers, failed denied lints (`wildcard_imports`, missing `const`, `items_after_statements`, and `unused_qualifications`), or encoded a runner-dependent test assumption. The final assumption expected a fixture marker in output bytes 0 through 63 even though libtest writes its harness name first under the serial campaign. The repairs restore bounded modules and strict imports, then bind the restart-output assertion to the exact persisted match offset and bytes rather than a harness prefix.
