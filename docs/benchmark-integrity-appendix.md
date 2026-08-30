# Benchmark integrity appendix

This appendix records benchmark cases where a higher score would not demonstrate a better coding
harness. It is a living companion to the exact evidence in the
[failure journal](../benchmarks/external/failure-journal.md). Final aggregates will be added only
after the frozen baseline and final-candidate campaigns finish.

## The rule

Peritus keeps the score produced by the unchanged benchmark. It does not read hidden verifiers or
reference solutions while solving a task, hard-code task names or private vocabulary, alter tasks,
fixtures, resources, deadlines, or scoring, or add behavior supported only by a benchmark-specific
win.

A low score is not automatically a benchmark gotcha. Real Peritus defects, ordinary model misses,
provider failures, and infrastructure failures remain failures. General corrections stay in the
product only when they improve ordinary coding work without relying on private benchmark facts.

Each entry below links to its published contract, hidden or contradictory expectation, exact
retained result, and evidence. The text here names the score-only shortcut Peritus refused.

## Broken or internally inconsistent evaluators

These cases cannot be satisfied reliably by following the published contract. Raising the score
would require changing the evaluator, relocating hidden data, exploiting its string processing, or
changing correct product behavior to accommodate an evaluator bug.

- [HBI-002](../benchmarks/external/failure-journal.md#hbi-002-task-011s-quality-score-cannot-reach-its-passing-threshold): the scoring formula cannot reach its own pass threshold. Refused: patching the formula or reporting an invented passing score.
- [HBI-003](../benchmarks/external/failure-journal.md#hbi-003-task-012-looks-for-ground-truth-above-the-task-directory): the oracle looks outside the task for ground truth. Refused: copying hidden data to the mistaken path.
- [HBI-007](../benchmarks/external/failure-journal.md#hbi-007-task-025-makes-one-required-rationale-term-unmatchable): normalization makes the required filename token unmatchable. Refused: unnatural text crafted to exploit the bug.
- [HBI-013](../benchmarks/external/failure-journal.md#hbi-013-task-032-treats-a-denied-commitment-as-a-promise): a denial is scored as a forbidden promise. Refused: removing a clear customer-facing safety boundary.
- [HBI-022](../benchmarks/external/failure-journal.md#hbi-022-task-042s-direct-oracle-cannot-load-ordinary-dataclasses): the direct oracle loads an ordinary dataclass module incorrectly. Refused: weakening normal Python code to fit the loader defect.
- [HBI-027](../benchmarks/external/failure-journal.md#hbi-027-harnessbench-relocates-mixed-model-sandboxes-without-rewriting-earlier-paths): the runner moves a sandbox without rewriting earlier evidence paths. Refused: rewriting immutable prior reports or patching the pinned runner.
- [HBI-035](../benchmarks/external/failure-journal.md#hbi-035-task-068-forbidden-claim-check-ignores-negation): a forbidden-claim check ignores negation. Refused: deleting explicit language that says the forbidden promise will not be made.
- [HBI-039](../benchmarks/external/failure-journal.md#hbi-039-task-073-omits-the-script-its-oracle-expects-to-audit): the oracle expects an input script absent from the fixture. Refused: fabricating the missing source or silently altering the fixture.

## Ground truth that contradicts the supplied inputs

These cases publish enough evidence to derive a valid answer, then grade against a hidden answer
that violates that evidence or the stated boundary. Winning would require knowingly returning a
wrong, unsafe, or contract-breaking result.

- [HBI-006](../benchmarks/external/failure-journal.md#hbi-006-task-024s-hidden-slots-conflict-with-the-supplied-calendars): every hidden meeting slot conflicts with supplied calendars. Refused: scheduling known conflicts.
- [HBI-012](../benchmarks/external/failure-journal.md#hbi-012-task-031-requires-a-reference-identifier-that-its-appendix-retires): hidden truth requires an identifier the appendix retires. Refused: reusing the prohibited identifier.
- [HBI-014](../benchmarks/external/failure-journal.md#hbi-014-task-033-calls-explicit-negative-evidence-insufficient): direct negative evidence is graded as insufficient evidence. Refused: discarding a sourced fact.
- [HBI-028](../benchmarks/external/failure-journal.md#hbi-028-task-056-ground-truth-contradicts-its-low-stock-boundary-rule): hidden stock membership violates the published threshold. Refused: adding an ineligible item.
- [HBI-036](../benchmarks/external/failure-journal.md#hbi-036-task-070-contradicts-its-shortlist-threshold-and-scans-raw-substrings): hidden shortlist expectations contradict the must-have threshold. Refused: weakening the stated hiring rule.
- [HBI-040](../benchmarks/external/failure-journal.md#hbi-040-task-074-ground-truth-contradicts-its-evidence-rubric): hidden grading contradicts the published evidence rubric. Refused: changing a rubric-faithful assessment to match the private grade.
- [HBI-041](../benchmarks/external/failure-journal.md#hbi-041-task-075-overlaps-its-confidence-calibration-categories): hidden confidence categories overlap contrary to the stated calibration. Refused: replacing a reasoned classification with the private label.

## Unpublished vocabulary, schemas, and serialization

These tasks request meaning or a general shape but grade exact private labels, tokens, aliases,
field spellings, row conventions, or prose. Consistently winning them would require reading the
hidden verifier or reference solution, or maintaining task-specific guesses. Peritus retains the
ordinary, evidence-grounded result instead.

- [HBI-005](../benchmarks/external/failure-journal.md#hbi-005-task-021-hides-its-error-taxonomy-and-row-number-convention): private error labels and row numbering.
- [HBI-008](../benchmarks/external/failure-journal.md#hbi-008-task-027-scores-hidden-wording-instead-of-contract-meaning): private risk and renewal wording.
- [HBI-009](../benchmarks/external/failure-journal.md#hbi-009-task-028-requires-unpublished-exact-prose): private to-do and approval phrases.
- [HBI-010](../benchmarks/external/failure-journal.md#hbi-010-task-029-requires-unpublished-issue-labels): private expense issue labels.
- [HBI-011](../benchmarks/external/failure-journal.md#hbi-011-task-030-requires-change-rows-for-unchanged-sections): private no-change row convention.
- [HBI-015](../benchmarks/external/failure-journal.md#hbi-015-task-034-requires-nonessential-and-exact-evidence-tokens): private evidence tokens and nonessential details.
- [HBI-016](../benchmarks/external/failure-journal.md#hbi-016-task-035-ignores-clear-priority-reason-synonyms): private priority verb set.
- [HBI-017](../benchmarks/external/failure-journal.md#hbi-017-task-036-double-counts-a-key-rename-and-requires-an-unpublished-duplicate-key): private key convention plus double-counted rename.
- [HBI-018](../benchmarks/external/failure-journal.md#hbi-018-task-037-requires-hidden-policy-quotations-and-mixed-authority-conventions): private quotation and authority conventions.
- [HBI-019](../benchmarks/external/failure-journal.md#hbi-019-task-038-rejects-real-input-paths-and-requires-unpublished-row-citations): private path and row-citation shape.
- [HBI-020](../benchmarks/external/failure-journal.md#hbi-020-task-039-uses-exact-substrings-for-equivalent-architecture-terms): private architecture substrings.
- [HBI-021](../benchmarks/external/failure-journal.md#hbi-021-task-041-rewards-one-unpublished-schema-version-field-spelling): private schema-version spelling.
- [HBI-023](../benchmarks/external/failure-journal.md#hbi-023-task-044-models-path-globs-and-prose-with-narrower-string-rules): private glob and prose string rules.
- [HBI-024](../benchmarks/external/failure-journal.md#hbi-024-task-045-awards-unpublished-raw-documentation-terms): private documentation tokens.
- [HBI-025](../benchmarks/external/failure-journal.md#hbi-025-task-047-grades-regression-tests-by-unpublished-raw-tokens): private test-source tokens.
- [HBI-026](../benchmarks/external/failure-journal.md#hbi-026-task-048-gives-fractional-credit-through-raw-release-note-substrings): private release-note substrings.
- [HBI-029](../benchmarks/external/failure-journal.md#hbi-029-task-057-oracle-requires-unpublished-state-and-log-encodings): private state and log serialization.
- [HBI-030](../benchmarks/external/failure-journal.md#hbi-030-task-058-conflict-check-requires-one-unpublished-location-specific-word): private conflict-location word.
- [HBI-031](../benchmarks/external/failure-journal.md#hbi-031-task-062-grades-an-unpublished-severity-taxonomy-and-exact-synonym): private severity taxonomy and synonym.
- [HBI-032](../benchmarks/external/failure-journal.md#hbi-032-task-064-requires-an-incident-identifier-absent-from-all-inputs): private incident identifier.
- [HBI-033](../benchmarks/external/failure-journal.md#hbi-033-task-066-supplies-no-blocker-severity-taxonomy): private blocker severity mapping.
- [HBI-034](../benchmarks/external/failure-journal.md#hbi-034-task-067-leaves-the-normal-evidence-status-value-unspecified): private normal-status enum.
- [HBI-037](../benchmarks/external/failure-journal.md#hbi-037-task-071-requires-unpublished-reply-keys-and-a-hidden-primary-clause): private reply keys and primary clause.
- [HBI-038](../benchmarks/external/failure-journal.md#hbi-038-task-072-requires-an-unpublished-compensation-token): private compensation token.
- [HBI-042](../benchmarks/external/failure-journal.md#hbi-042-task-076-uses-adjacency-only-checks-for-correct-administrative-wording): private adjacent-word sequence.
- [HBI-043](../benchmarks/external/failure-journal.md#hbi-043-task-077-leaves-archive-chain-and-prose-serialization-unspecified): private archive-chain serialization.
- [HBI-044](../benchmarks/external/failure-journal.md#hbi-044-task-079-keeps-source-identifiers-under-an-unpublished-normalized-schema): private normalized source schema.
- [HBI-045](../benchmarks/external/failure-journal.md#hbi-045-task-080-requires-an-unpublished-conflict-source-key-alias): private conflict-source alias.
- [HBI-047](../benchmarks/external/failure-journal.md#hbi-047-task-086-makes-two-checks-depend-on-one-unpublished-reason-phrase): private orphan-reason phrase.
- [HBI-053](../benchmarks/external/failure-journal.md#hbi-053-task-092-leaves-severity-reject-priority-and-summary-shape-unpublished): private severity, priority, and summary shape.
- [HBI-054](../benchmarks/external/failure-journal.md#hbi-054-task-093-assumes-cross-session-campaign-carryover-and-duplicate-bot-routing): private cross-session carryover and duplicate routing.
- [HBI-063](../benchmarks/external/failure-journal.md#hbi-063-task-102-refusal-vocabulary-excludes-an-ordinary-supported-phrase): private refusal synonym set.
- [HBI-066](../benchmarks/external/failure-journal.md#hbi-066-task-105-partial-snapshot-and-ledger-shapes-are-unpublished): private partial-snapshot and ledger shapes.
- [HBI-067](../benchmarks/external/failure-journal.md#hbi-067-task-106-pending-action-alias-repetition-is-unpublished): private repeated alias convention.

## Hidden runtime and effect conventions

These cases require an unpublished mechanism rather than a published artifact contract. The
score-only shortcut would be private-verifier inspection or adding an otherwise unjustified effect.

- [HBI-046](../benchmarks/external/failure-journal.md#hbi-046-task-081-requires-a-redundant-root-http-request): the exact local DOM is supplied, but scoring demands a redundant root request. Refused: extra network traffic whose only purpose is the hidden trace check.
- [TBI-004](../benchmarks/external/failure-journal.md#tbi-004-the-windows-control-verifier-requires-an-unpublished-socket-path): the verifier requires a private socket path absent from the task. Refused: reading the verifier or hard-coding its path.
- [TBI-005](../benchmarks/external/failure-journal.md#tbi-005-a-hidden-model-call-signature-cannot-be-recovered-from-a-state-dictionary): the checkpoint does not reveal the hidden two-argument model signature. Refused: reading the verifier or reference implementation and hard-coding its private interface.

## Cases deliberately excluded from this appendix

Entries labeled `HBF` or `TBF` are product or adapter defects and are fixed generally when the
evidence supports a correction. `HBM` and `TBM` entries are ordinary model limitations. Clean
control tasks remain unchanged. Infrastructure failures such as unavailable dependencies, provider
terminals, container failures, and verifier timeouts are reported separately and never converted
into an inferred score.

The final delivery report will add the frozen before/after aggregates and link each demonstrated
general correction to its unchanged rerun. It will not delete, reclassify, or rewrite a retained
failure merely because a later candidate performs better.
