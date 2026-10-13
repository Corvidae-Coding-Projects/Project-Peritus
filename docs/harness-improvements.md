# Harness improvement inbox

Ordinary runs can collect **untested suggestions**, with immutable references to the runs that
motivated them. Collection makes no model calls, generates no patches, and does not change the
running harness. Failed runs and runs requiring at least three cycles supply investigation
suggestions. These signals do not prove that the harness is defective. You can also save a specific
suggestion against any completed run in the same workspace.

In the GUI, open **Improvement inbox** from the command directory or enter `/improvements`.
Inspect the retained evidence, add a suggestion, or dismiss an irrelevant suggestion. Repeated
observations are deduplicated; dismissal survives later runs and daemon restarts. Evidence is
bounded and includes its source run, observed outcome, Peritus package version, and a digest of the
retained observation. Open the source run to inspect its full history.

## Explicit evaluation

Register a Git checkout of the Peritus source repository as a workspace and open that project in
the GUI. Select it in the inbox's **Peritus source workspace** control, then choose
**Generate patch & evaluate** on a suggestion. An ordinary application repository is rejected as
the evaluation target. Evaluation uses configured providers and the existing bounded
writer/checks/reviewer/fixer pipeline, producing an isolated candidate patch.

Evaluation creates a durable workbench and records the selected candidate, each immutable run
observation, and the evaluation directive as separate queue inputs. The directive explicitly
depends on the candidate and evidence inputs. New evaluations explicitly use the real-testing
skill: reproduce through the actual program, compare the same operations against baseline and
candidate, and retain a real-interface failure replay. Automatic conventional test and qualification suites excluded by
that governing request are retained as `NOT EVALUATED` observations, not successful checks.
Actual baseline/candidate operations, internal artifact/source checks, and independent review
provide the selected acceptance evidence. Requests without those exclusions retain their normal
check plans. Evaluations do not generate or run unit tests,
mocks, formal verification, or qualification suites. The normal runner owns execution,
cancellation, recovery, and review. Inspect its actual commands, outputs, and artifacts:
the requested baseline comparison is not itself a measurement or a
guaranteed statistical improvement. An unreproduced suggestion concludes without a patch only
after successful real-interface observations and a fresh independent review against unchanged
source and the current public request. This is `CompletedWithoutCandidate`, not candidate
acceptance; no candidate export, accept, commit, or promotion controls are granted.
The no-patch terminal states require application protocol 2.1. Update the CLI, TUI, WebUI server,
and daemon together; protocol 2.0 clients receive an incompatible-version negotiation result
before any new settlement tags are sent. Durable conversation and run identities are retained.

The inbox retains the original durable conversation, run, and target identities before execution
admission. Repeated evaluation requests replay the same deterministic create, queue, and start
operations rather than starting another evaluation. If preparation was interrupted, open the
evaluation workbench to inspect the retained queue, then choose **Resume evaluation request** with
the same target and provider. If a run already exists, use its normal retry/cancel controls.
Restarting the daemon or opening the inbox never resumes evaluation automatically.
The directive version is frozen with its reservation so an upgrade cannot change an already
acknowledged queue operation. Legacy reservations retain their original directive and identities;
new suggestions evaluated after the upgrade use the real-testing directive.

Review the patch, checks, and reviewer findings in that conversation. Use the existing explicit
candidate export/accept/commit controls when satisfied. These changes remain source changes;
installing a rebuilt harness remains an explicit release/update operation. Existing runs and the
installed harness are not rewritten. The inbox does not bypass F0's promotion authority or claim
that a coding run is an F0 evaluation campaign. F0's statistical promotion and production-pointer
machinery remain a separate domain surface.

## CLI

All clients share the daemon's inbox. Supply the configured daemon endpoint as usual:

```sh
peritus improvements list --workspace WORKSPACE_ID
peritus improvements suggest --workspace WORKSPACE_ID --run SOURCE_RUN_ID --proposal 'Investigate repeated tool-schema failures'
peritus improvements dismiss --workspace WORKSPACE_ID --candidate CANDIDATE_DIGEST
peritus improvements evaluate --workspace WORKSPACE_ID --candidate CANDIDATE_DIGEST --target PERITUS_WORKSPACE_ID --provider PROVIDER_ID --run NEW_RUN_ID
peritus runs show --run EVALUATION_RUN_ID
```

`--json` includes proposal text, evidence digests/run IDs, dismissal state, and the original
evaluation conversation, run, and target workspace. Source workspace, target workspace, and
provider IDs are never inferred from a different run. GUI evaluation uses the target daemon's
first configured provider for all roles; the resulting conversation supports the existing model
controls.

Suggestions live in `STATE_ROOT/improvements.sqlite3`, separately from executable harness
configuration. Writes are durable SQLite transactions. Pre-release schema 1 is preserved in
`improvements-quarantine` before a fresh schema 2 inbox is created. Other unsupported versions
are rejected; malformed or digest-mismatched records report errors rather than silently emptying
the inbox. The inbox returns retained suggestions, with dismissed suggestions hidden by default
in the GUI. It has no 32-suggestion or four-evidence-run display cap. Wire and per-record storage
bounds still apply. A selected suggestion's evidence is frozen for its evaluation. Automatic
collection excludes reserved evaluation run identities across source and target workspaces,
including failure settlement and restart backfill, so evaluation does not collect its own results
as new investigation suggestions.
