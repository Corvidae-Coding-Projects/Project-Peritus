# Upstream read-argument correction: issue #160 follow-up

The prior fix (`02ec23afaf945859631a801a2692d635aa6136da`) preserves the current task after
rejected reads. This follow-up addresses the earlier failure: an OpenCode proposal with no
required `path` was accepted by the adapter and dispatched to the host, which correctly
rejected it. The original reporter's raw provider reply remains unavailable; its exact cause
cannot be established from the screenshot alone.

## Source correction

OpenCode already receives each declared tool's full schema. However, the final prompt also
demonstrated generic `"arguments": {}`, and the decoder checked only tool names, selection,
call count, and object-valued arguments. It did not check required fields or basic field types.

- The generic empty-argument example is replaced with schema-specific guidance and a valid
  read example. Empty arguments remain valid for tools whose schema permits them.
- After JSON/DSML normalization and selection checks, the decoder compares proposed arguments
  with that exact declared tool's top-level required fields and direct property types.
- A completed inert inference with a missing/wrong-type field gets precise feedback naming
  the tool and field. The existing correction attempt regenerates the next turn from the
  original conversation before any proposed tool is dispatched.
- No path is inferred, defaulted, unwrapped from another shape, or copied from another task.
  The model must provide the corrected value or request valid authorized discovery.
- If a valid-envelope argument mistake persists after correction, the host receives its
  normal actionable schema rejection and continues. This adds no terminal argument blocker
  or unbounded vendor retry loop. Native failures, ambiguous exits, permissions, and full host
  schema validation retain their existing ownership and behavior.
- Correction inference usage is combined with the original inference using the existing
  accounting path. Rejected raw replies remain bounded private audit evidence.

Only `peritus-provider-opencode` behavior changes. The original context-continuity fix remains
in place for any tool/provider failure that reaches the host.

## Observable contract

The checker defines these expectations before execution:

1. On the baseline, a real model-authored `workspace_read {}` reaches the host and gets an error.
2. On the changed adapter, one such proposal triggers real correction inference; no malformed
   host read is dispatched, and the current inspection task finishes.
3. Missing fields and wrong declared types are corrected from the current task, with no
   invented file paths or silent argument coercion.
4. A second malformed proposal can receive ordinary host rejection and recover without a
   terminal provider error or task switch.
5. Valid empty arguments and ordinary non-faulted requests remain usable.
6. Final reports contain independently created payload identifiers and structures observed
   by reading the actual files. The returned history must account for every live tool call;
   omitted history cannot establish a zero host-error count.

## Real execution

Linux x86-64; Rust 1.97.1; workspace dependency lock; two build workers. The saved public-library
consumer operates the actual developer loop, memory tools, durable context, filesystem tools,
and official OpenCode client using Zen / `deepseek-v4.1-flash`.

For controlled failure input, the wrapper temporarily replaces only the disposable router's
inference instructions and asks the **real model** to emit the malformed proposal. It restores
the original instructions afterward. Native tools remain disabled/denied, credentials remain
private, and vendor stdout/stderr/status are unchanged. No provider response or host read result
is fabricated. Captures prove both the malformed model output and the actual correction request.

| Scenario | Observed result |
| --- | --- |
| Baseline empty read arguments | PASS reproduction: 1 failed host call, no argument-correction inference, then task recovery; 3 provider requests |
| Changed empty read arguments | PASS: 1 argument-correction inference, 0 host errors, correct report; 3 provider requests |
| `path: null` | PASS: corrected to actual current-task file path; 0 host errors |
| Nested `arguments.path` with required top-level path absent | PASS: model regenerates proper arguments; 0 host errors |
| String-valued `start_line` | PASS: declared integer type correction; 0 host errors |
| Recognized plain DSML/XML empty read | PASS: normalized then argument-corrected before dispatch; 0 host errors |
| Two consecutive malformed proposals | PASS: 1 correction, then 1 ordinary host rejection, followed by successful task completion; 4 provider requests |
| Valid `skill_list {}` | PASS: actual skill discovery, no argument correction or host error, then successful inspection |
| No fault injection | PASS: ordinary inspection, no correction or host errors |
| Official Codex / `gpt-6.1-sol` control | PASS: existing task-continuity live recovery scenario completes |
| Saved task-continuity generator | PASS: all 24 actual tool/context recovery journeys |

There are eight distinct changed-program live OpenCode scenarios, plus a baseline reproduction
and the independent provider control. Every inspection report contains `alpha.json` /
`ALPHA-7391` / `fortress` and `beta.json` / `BETA-2846` / `village`, matching files created before
execution. The current task finishes in each case.

An initial special-token DSML fault prompt was altered by the model into unsupported markers.
It exercised successful format correction, not the intended required-field branch, and was not
counted as that branch passing. A recognized plain-markup input then exercised the argument
branch successfully. The initial ungrounded baseline similarly hit the existing tool-selection
correction; the final replay first completes actual grounding so it isolates argument repair.

The adapter/host code was built and run. `cargo fmt --all -- --check` and strict
`cargo clippy --offline --jobs 2 -p peritus-provider-opencode --lib --all-features -- -D warnings`
passed. No unit tests, fake-provider suite, formal verification, or repository qualification
suite was used. The final diagnostic wording was also exercised in the final empty-read,
markup, repeated-error, and valid-empty live runs.

## Replay and boundaries

See [`reproducers/open-issues/README.md`](reproducers/open-issues/README.md) for build/launch
commands. Inputs, captured genuine vendor output, original/corrected inference input, durable
context, reports, and JSON results remain outside the repository under
`/home/doll/.cache/peritus-issue160/argument-*`.

The new check covers missing top-level required fields and direct declared property types;
it is not a general JSON Schema implementation. Nested constraints, ranges, authorization,
filesystem existence, and source freshness still belong to the existing host checks. Model
mistakes cannot be eliminated universally, but the observed malformed-read path now has a
tested correction before host dispatch and a tested nonterminal recovery when it persists.
No installed binaries, user daemons, active projects, or user sessions were changed.
