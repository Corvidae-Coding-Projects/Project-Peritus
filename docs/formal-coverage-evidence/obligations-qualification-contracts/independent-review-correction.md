# Correction to obligations qualification review

The remaining extraction-error gap is on the production-called verified kernel `RequirementLedger::extract_preimage`: its successful result has the exact `spec_refines_extraction` contract, while its error branch is unconstrained in the reviewed snapshot. The public `RequirementLedger::extract` wrapper is compiled under `cfg(not(verus_only))`; it calls that kernel, passes its error through, and then performs the ordinary SHA-256 step. I source-reviewed and runtime-log-inspected that wrapper correspondence, but did not claim a direct Verus contract on the ordinary wrapper or a proof of SHA-256 execution.

This clarification does not change the bounded PASS for the separate `qualify` 56-file increment or any source identity.
