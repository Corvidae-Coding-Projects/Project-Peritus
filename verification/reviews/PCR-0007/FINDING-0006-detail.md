# FINDING-0006 — nested aggregate xtask smoke test exhausted Windows CI timeouts

Original severity: medium, blocking.

Disposition: fixed.

The black-box member-directory discovery test invoked `xtask all`, causing the full aggregate policy path to run from inside the xtask test target. Two independent Windows CI jobs completed 429 normal tests and then reached the 15-minute job timeout in this test, making the failure deterministic rather than an intermittent runner fault.

The reviewed candidate invokes `architecture-check`, which is sufficient to exercise workspace-root discovery from the xtask member directory, and asserts the command's success output so a no-op cannot pass. No product source or proof-impact source changed. The focused CLI target passes all four tests, the complete xtask all-targets suite passes 465 tests, and strict Clippy and formatting checks pass.
