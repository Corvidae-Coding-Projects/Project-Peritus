# crates/app/peritus-product-runner/examples — navigation

## Responsibility

Direct native consumers of ProductRunner, including credentialed disposable-workspace examples; read each entrypoint before running it.

## Files in this folder

| File | Role |
| --- | --- |
| [live_account_product_run.rs](live_account_product_run.rs) | Credentialed end-to-end writer/reviewer/fixer qualification on a temporary Rust repository. |
| [live_design_expansion.rs](live_design_expansion.rs) | Credentialed qualification for zero-question expansion, exact reuse, material answers, steering, guided Design without workspace edits, and browser-sized expert passes. [live_design_expansion/browser.rs](live_design_expansion/browser.rs) supplies the browser source/recovery cases and independently checks an actual authorized Rust edit. |

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
