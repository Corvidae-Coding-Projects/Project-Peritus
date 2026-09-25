# Independent issue 84 product reliability, CI, and proof-impact review — PCR-0007

Decision: **APPROVED**

Reviewer: `ACTOR-0008`

Reviewer principal: `Corvidae-Coding-Projects/Project-Peritus/session/61/task/root/pcr7_final_review_sol`

Candidate commit: `d7f710e6e0da2c7577a0d7af0f89f376b01a418f`

Candidate tree: `7d077975141a4ab6df6400f6eda34baf8d522390`

Prior approved candidate: `1440209e50c851b228e6948db3e5389e22bcc55b`

Authorization base: `958e3f078eca5031427053710c771aef536d6195`

Reviewed at: `2026-09-16T18:49:17Z`

The protected base and prior approved candidate are ancestors of this candidate. Independent review inspected the complete one-file delta from the prior candidate, the unchanged product and proof-impact source inventory, the exact checker test, the retained package gates, and the complete six-finding ledger. No material product, checker, gate, transition, normalization, or finding-record defect remains.

The only change from the prior approved candidate is `xtask/tests/cli.rs`. The member-directory discovery test previously launched the broad `all` policy command from inside the xtask test target. Two independent Windows jobs passed 429 normal tests and then reached their 15-minute timeout in this nested aggregate check. The candidate uses the root-dependent `architecture-check` command and asserts its success message, preserving the test's workspace-discovery purpose while keeping execution bounded. An independent exact-candidate run passed all four CLI tests in 3.01 seconds. The supplied complete validation reports 465 passing xtask tests across all targets, strict Clippy success, and formatting success.

No file under the product source, public protocol artifacts, workspace manifests, or lockfile changed from the prior candidate. The proof-impact inventory is byte-identical. The exact final3 plan still contains 366 source transitions affecting 66 packages: 53 class H, 12 class V, and one class T. Its independently recomputed transition digest is `088ab4e4e6f414dc0a58ea0312aadc03a2d9737968110f313a1acc0957a02d8c`. Every one of the 366 current transition hashes also matches this candidate tree.

The 132 normalized proof-impact package gate logs are carried forward from the prior approved candidate because every covered source byte is unchanged. They bind commit `1440209e50c851b228e6948db3e5389e22bcc55b` and tree `86fd9fe3d21a79655a02234ddc96c33515c6ad1f`; they are not represented as executions of the new xtask-only commit. All 132 commands passed: 66 ordinary test commands and 66 class-correct Verus commands. The retained records contain 2,247 passed tests, zero failed tests, and six ignored tests. All 66 Verus commands succeeded; five explicit result blocks recorded 177 verifications and zero errors, while 64 commands reused successful cached verification.

The finding ledger is complete:

1. The high-severity restart replay risk is fixed. A persistence failure cancels live work, restored unfinished runs require recovery, and only explicit retry starts new provider or tool work.
2. The medium-severity invisible `ChatOpen` error is fixed. The TUI restores input state and displays the daemon's actionable diagnostic.
3. The medium-severity negotiated diagnostic limit failure is fixed. UTF-8-safe truncation occurs before framing and preserves machine-readable error fields.
4. The medium-severity classification and schema contradictions are fixed. Git prerequisites report `Workspace`, unsupported effort reports `Provider`, the authoritative schema contains both tags, generated projections agree, and public wire regressions cover tags 10 and 11.
5. The high-severity linked-worktree selection bug is fixed. Baseline resolution uses the selected worktree's Git directory and root, its relocated divergent-primary regression remains token-identical, and the installed probe selected the intended source.
6. The medium-severity deterministic Windows timeout is fixed. The xtask member-directory smoke test now runs one bounded root-dependent command and asserts its output instead of invoking the aggregate `all` path.

The source-budget repair remains mechanical: production bytes and test assertions are unchanged, and both formerly oversized production source files remain below the 400-line repository limit. The new commit changes no product behavior and does not alter any of the first five fixes.

Gate and finding artifacts contain no trailing spaces or tabs. Four ignored tests are subprocess fixture entry points exercised by parent regressions. Two ignored daemon tests require Xvfb, xdotool, ImageMagick, and Tk and do not cover these findings. Verus emitted coverage warnings for derived `Clone` specifications but no verifier error. Sixty-four Verus logs are successful cache-only invocations without fresh proof-count lines. TUI and launcher remain outside the proof-impact package-gate set and were separately focused-tested and independently reviewed. Raw Windows CI transcripts and the installed DeepSeek probe transcript are not included in this artifact directory.

This approval authorizes only the exact source delta and carried-forward evidence described above. Existing proof obligations retain their current status; this review discharges none of them and does not itself authorize a merge or release.
