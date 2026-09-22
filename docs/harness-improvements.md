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

The evaluation task calls for reproducing the problem first, checking a regression against the
baseline, generating a patch only when justified, and reporting the candidate's checks. The normal
runner owns execution, cancellation, recovery, and review. Inspect its actual test output: the
task's requested baseline comparison is not itself a measurement or a guaranteed statistical
improvement. An unreproduced suggestion may conclude without a patch.

The inbox retains the original evaluation identity before launch. Repeated evaluation requests
return that run, rather than starting another. If launch was interrupted before the run was
created, explicitly choose **Resume evaluation request** with the same target and provider. If a
run already exists, open it and use its normal retry/cancel controls. Restarting the daemon or
opening the inbox never resumes evaluation automatically.

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
evaluation run ID. Source workspace, target workspace, and provider IDs are never inferred from a
different run. GUI evaluation uses the target daemon's first configured provider for all roles;
the resulting conversation supports the existing model controls.

Suggestions live in `STATE_ROOT/improvements.sqlite3`, separately from executable harness
configuration. Writes are durable SQLite transactions. Unsupported versions or malformed/digest-
mismatched records report errors rather than silently emptying the inbox. The inbox shows up to 32 active suggestions and fills remaining space with recent dismissed
suggestions. Older dismissed records remain in storage. Each suggestion retains four evidence
runs; dismissing a suggestion makes room when the active inbox is full. A selected suggestion's evidence is frozen for its evaluation.
