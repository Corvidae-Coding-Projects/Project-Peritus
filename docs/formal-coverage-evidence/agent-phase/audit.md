# Agent lifecycle phase checkpoint

Production reduction and replay now call `command_phase_transition` with the actual
`AgentCommandKind`, current phase, provider-in-flight fact, and completion-presence fact.
The verified result specifies the exact successor or rejection for every non-control command.
Pause, resume, cancellation request, and cancellation completion still use their separately
verified production control functions. The outer reducer commits the computed phase after
command-specific updates succeed.

The older integer-tag relation is explicitly only an existential projection: it says that
some command could connect the two phases. It is not used as the production admission rule.
Exact tag conversion, command-specific phase checks, retry/completion prerequisites, and
terminal failure rejection are verified without new executable preconditions or assumptions.

A regression probe reproduced a real replay defect against base commit
`1aa9282ff2cfcbf8fac325f11f157f408d33ee30`: replay accepted a correctly fenced Failed event
after a terminal event. Reduction already rejected that command. The production phase kernel
now rejects late Failed and Exhausted commands in both paths. The baseline probe fails on
the first Failed case; the repaired test covers both variants. This increment is therefore
not described as entirely behavior preserving.

Moving the actual command vocabulary into verification exposed previously unconstrained
derived Clone implementations. The replacement implementations prove every scalar field,
all sequence contents and order, optional/variant identity, and capability string/byte views.
They cover SafeText, AgentFailure, provider observations, completion proposals, tool proposals
and results, and the complete command vocabulary. No warning suppression was added.

Strict pinned Verus passed 56 checks with zero errors and no warnings. The all-feature suite
passed 84 tests; the no-default-feature suite passed 76. Both all-target strict Clippy
configurations and formatting passed. Parent source review checked the exact 29-file
checkpoint, production call path, complete clone-field coverage, preserved rejection metadata,
regression evidence, and the stated limits. Earlier independent parent tests cover the
control/phase integration; the final feature-suite results here are the implementer's runs.

This does not prove complete payload validation, digest correctness, limits, tool execution,
provider truth, durable replay fencing, or recovery effects. The booleans supplied by the
ordinary reducer remain a correspondence boundary for those wider guarantees. The retained
checkpoint is independently reviewed source and local evidence, not a whole obligation
discharge, protected CI approval, clean final commit, or complete compiler-input attestation.
