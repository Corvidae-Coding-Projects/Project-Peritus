# Stage 6 security-readiness decision contract

`stage6::security` calls the public `peritus_security_policy::evaluate_security_readiness`
with canonical, synthetic `SecurityEvidence` for one exact `IntegratedCandidate`.
Its separate oracle constructs the ordered `UnmetSecurityCondition` sequence from
the case flags, then compares that sequence and the final ready disposition.
The complete-evidence case must be ready. Generated cases select one requirement,
criterion, inventory, and artifact role each, then independently vary missing,
failed/incomplete, empty digest, and stale candidate states. Review cases vary
completion, actor and organization independence, report digest, required scope,
and candidate binding; one finding varies unresolved high severity, accepted
risk, resolved high severity, and open medium severity. Mixed cases ensure
that independent faults remain visible in stable family order.

The fixture builders use public constructors and literal synthetic IDs and
digests. The expected result does not inspect the evaluator's internal check
flags or call its phase helpers. It compares exact diagnostic variants and
positions, rather than readiness alone. The target is bounded to one altered
observation per flat family and at most one finding; it does not explore
duplicate/noncanonical evidence construction errors, multiple concurrent
findings, all candidate tuple dimensions, or release-policy authority.

The disposable mutation in `calibration/stage6-security.patch` ignores failed
criterion outcomes. The mixed fixed example and generated failed-criterion
cases should catch it.
