# Independent Windows diagnostic portability and proof-impact review — PCR-0008

Decision: **APPROVED**

Reviewer: `ACTOR-0009`

Reviewer principal: `Corvidae-Coding-Projects/Project-Peritus/session/61/task/root/pcr8_final_review_sol`

Candidate commit: `58da2dbc18f3248594e3ea3c7b67b540723b4b60`

Candidate tree: `fd1a8f2a9bb48d55fda7128aa135632b7aa69be5`

Authorization base: `ee31669a2da6d4e91a17553aa92eb27b28da8f33`

Reviewed at: `2026-09-17T00:17:43Z`

Independent read-only review inspected the exact three-file candidate delta. The production persistence formatter is byte-identical to its parent. The test derives the expected cause text from the same `std::io::Error` passed into `ProductRunServiceError::persistence`, so it verifies complete cause preservation on each platform. The separate operation-context and recovery-action assertions remain unchanged.

The reviewer confirmed that `ACTOR-0009` is a fresh canonical principal for this exact review. The actor registry grows from eight entries to nine; historical actor fields are unchanged except for their required whole-provenance digest refresh, and all eight historical provenance records remain byte-semantically unchanged.

The original low-severity blocking finding is fixed: two Windows jobs rejected a Unix-specific `Permission denied` literal for raw OS error 13. Focused Linux and Windows/Wine executions passed after the repair. The complete error-test module passed, strict daemon Clippy passed, formatting and diff checks passed, and the candidate actor checker’s adversarial append/rewrite/reuse tests passed.

Following the approved sequencing merge, the candidate was rebased from commit `eb187b0f0b1dafb1f079d34aa581acfcd0b6239e` onto protected authorization base `ee31669a2da6d4e91a17553aa92eb27b28da8f33`, producing commit `58da2dbc18f3248594e3ea3c7b67b540723b4b60`. The protected base is the rebased candidate’s direct parent. The old and new candidates have the identical implementation tree `fd1a8f2a9bb48d55fda7128aa135632b7aa69be5`, and the complete three-file binary delta is unchanged.

The 132 successful package-gate logs are retained as executions of `eb187b0f0b1dafb1f079d34aa581acfcd0b6239e`, not represented as executions of the rebased commit. Every log records that old commit and the shared tree `fd1a8f2a9bb48d55fda7128aa135632b7aa69be5`. Because the complete implementation tree and every tested repository byte are identical, the 66 ordinary-test results and 66 Verus results remain valid evidence for the rebased candidate. The `peritus-tcb` Verus command retained its reviewed exception and omitted `--no-cheating`; every other Verus command required `--no-cheating --rlimit 20`.

The detached verdict binds the protected authorization base, rebased implementation commit, and unchanged implementation tree. No actor, provenance, source-shape, gate, or product-behavior defect remains. This approval authorizes only the three exact source transitions and does not change any existing proof-obligation status.
