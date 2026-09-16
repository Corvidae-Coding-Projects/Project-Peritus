# Exact knowledge delta packets

The parent reviewed the twelve changed files and their production correspondence, then reproduced
214 strict Verus results with zero errors and all 20 package tests in an isolated copy of the exact
30-file package. Integration preserved those hashes and reproduced the same results. Both context
reuse integration tests also passed. The parent-authored material/authority helper portion received
a separate independent Sol source review before the combined increment was accepted.

## Proved successful admission and complete result

The actual public `plan_delta_packet` succeeds exactly when the roles agree, the current candidate
matches every supplied identity field including checkpoint sequence, every current section is fresh
under the normalized same-revision observation, and prior clarification targets are valid. Current
freshness is a quantified input predicate over lineage, role, creation sequence, source identities
and digests, and the section kind's conversation/candidate dependencies.

An input-only deterministic prior-plan model preserves exact section identities and decisions,
including dependency invalidation. A separate correspondence proof connects that model to the
existing actual planner's exact-prefix contract; another induction connects the normalized current
plan's all-reuse result to the quantified freshness predicate. Admission does not depend on an
unconstrained hypothetical output plan.

Every packet position corresponds to the same current section. Navigation-only kinds produce
Navigation; other sections produce CurrentReference exactly when the actual prior plan reuses that
identity and the first prior identity match retains identical material. Otherwise they produce
ChangedFact. Material includes kind, all section-digest bytes, ordered source identities/digests and
ordered dependency identities. Separate candidate/currentness checks remain mandatory.

The packet retains its exact role and complete candidate identity. All ordered entries, all three
delivery counts, their partition of the entries and the invalidated-prior count match the input
model. Actual public getters and the complete packet Clone retain those fields. Actual counts are
proved safe within the loop's finite length; no new public execution precondition was introduced.

## Runtime correspondence and validation

The change keeps the existing role, candidate, current-freshness and prior-target error order, and
preserves navigation-first delivery priority. Derived candidate and material comparisons were
replaced with exact executable comparisons over the same underlying fields. The candidate helper's
same-candidate relation includes run, workspace, digest and conversation; checkpoint equality is
checked separately. The semantic model preserves all of those fields.

Four new public tests exercise admission precedence, late candidate/section/source identity bytes,
changed dependency material, navigation authority, complete output accounting and cloning. Default
and all-feature implementation tests each passed 20 cases. Implementation strict Clippy and
formatting passed. Its source-layout check passed on the isolated workspace at that checkpoint;
this is not a final shared-worktree layout result.

The actual `peritus-context` consumer reads the packet's exact getters and checks snapshot identity,
links, node digests, role visibility and delivery/authority compatibility before constructing
selections. That source path was inspected and both existing reuse-matrix tests passed after
integration. This does not yet prove the complete context construction or rendering path.

## Review limits

Exact public error payloads, remaining general constructor admission/lineage/time/bound invariants,
context selection/rendering, persistence and end-to-end product resume remain follow-up work.
Supplied digests and observations do not prove external file authenticity or execution. The
private helper preconditions are discharged by their production callers; no public restriction,
assumption, axiom, external body, alternate executable implementation or lint suppression was added.

The root package no longer emits its earlier DeltaPacket Clone warning. Dependency Clone warnings
and existing automatic-trigger notes are retained in raw logs; this is not a warning-free workspace
claim. Independent review of the parent helper inspected its supplied 182-result log but did not
rerun tools; the complete 214-result package was subsequently independently run by the parent.
This record is a reviewed production-proof increment, not an obligation discharge or final hosted
CI qualification.
