# Knowledge invalidation planning correspondence

The actual plan_invalidation implementation now proves successful output contains exactly one
entry per supplied section, in the same order, with the exact direct decision precedence and
propagation from previously planned dependencies. Reuse and invalidation counts exactly partition
the result. The direct reuse predicate checks complete lineage, role, creation sequence, source
identity/digest membership, named clarification effects and kind-dependent conversation/candidate
freshness. Final reuse for each entry is equivalent to those direct facts plus no invalidated
matching dependency among the prior entries. Clarification target validation and public source,
section, target and plan membership lookups have exact supplied-data contracts. Actual Clone
implementations retain the complete semantic fields and ordered collections used by these models.

Parent read all 12 sources, the specification models, actual delta-planner and product-resume
callers, and the public regression matrix. All frozen source hashes matched and were saved before
the next increment. An independent source tree reproduced 162 strict pinned Verus checks with
zero errors and all 14 package tests. Implementer default/all-feature tests, package-local strict
Clippy, formatting and source-layout passed. Reverse consumer tests for context and product runner
passed; their combined Clippy command failed on concurrently edited scheduler helpers and is not
a complete integration Clippy pass. One existing delta.rs non-Copy Clone warning remains.

This checkpoint does not prove a complete transitive dependency closure theorem. The model
propagates invalidation from prior entries, but snapshot construction currently lacks a verified
canonical/topological invariant connecting every dependency to a unique earlier section. Parent
review identified this limit and the next increment is proving the actual constructor and
backward dependency relation, then connecting it to transitive closure. The public decision getter
also currently specifies only reuse classification rather than the exact invalidation reason;
its strengthening is included in that followup. These are feasible open proof obligations, not
exclusions or demonstrated unsupported Rust.

Delta selection, authority classification, context rendering, hashing, persistence/reload and
truth of supplied observations are not proved by this planner increment. No behavior bug was
reproduced in this slice. This retained review is independent source evidence for a historical
checkpoint, not an obligation discharge, protected approval or final clean-commit CI attestation.
