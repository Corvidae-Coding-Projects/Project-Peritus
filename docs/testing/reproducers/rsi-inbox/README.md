# Native RSI inbox failure replay

This observer connects through the real local daemon IPC protocol and reads the public revisioned input queue. It supplies no counterfeit workbench or provider result.

Use a disposable daemon state and Git-backed copy of Peritus. Collect a suggestion against an actual terminal source run through `peritus improvements suggest`, then choose **Generate patch & evaluate** in the browser or invoke `peritus improvements evaluate`. Record the admitted run ID. The source workspace, target workspace, and provider must be configured in that disposable instance.

```sh
python3 docs/testing/reproducers/rsi-inbox/build_observer.py --output /tmp/peritus-rsi-observer-build
python3 docs/testing/reproducers/rsi-inbox/check_directive.py --observer target/release/peritus-rsi-observer --endpoint FIXTURE_SOCKET --run NEW_EVALUATION_RUN
```

Before the fix, a new evaluation queues `create the smallest complete harness patch and regression tests` and `Run relevant existing tests`, without a governing real-testing-only restriction. The checker exits nonzero. New evaluations on the fixed version queue the real-testing skill and explicit exclusion; the checker passes. Legacy reservations deliberately retain their original exact queue payloads and identities across the upgrade, so they are not valid inputs to this new-directive acceptance check.

The separate browser regression is observable without internal APIs: collect at least 33 distinct suggestions against a terminal run. Before the fix, the inbox says it is full and requires dismissal while the actual backend accepts another suggestion. On the fixed UI, retained suggestions remain collectible without the false full-inbox warning. Dismissal still hides a suggestion, and **Show dismissed suggestions** reveals it.

The executed fixture and raw before/after evidence are under `/home/doll/Peritus-RSI-2026-10-12/native`. See [the campaign report](../../rsi-real-testing-2026-10-12.md) for the actual run identity, artifact provenance, commands, and bounded coverage. This fixture is separate from the installed user daemon and its active browser-building run.

The same launcher also builds `target/release/peritus-rsi-plan`, a real public consumer which discovers the check plan from the supplied checkout. It checks the normal plan, explicit suite selection, and the refusal to pass missing required results without fabricating execution records:

```sh
target/release/peritus-rsi-plan /home/doll/Peritus-RSI-2026-10-12/peritus-source crates/app/peritus-cli/src/runner.rs
target/release/peritus-rsi-plan /home/doll/Peritus-RSI-2026-10-12/peritus-source crates/app/peritus-cli/src/runner.rs real-interface
```

In the actual native run, inspect its `checks` output and provider tool receipts: the excluded conventional commands must say `NOT EVALUATED`, actual baseline/candidate CLI operations must retain their observations, and independent review must still complete. A printed command in an excluded observation is not an executed command.

For the no-patch regression, collect the false claim “The Peritus CLI --help omits --version even though --version is a supported option” against a terminal source run, evaluate it in the disposable Peritus checkout, and let the actual worker operate the CLI. Before the repair it fails before review with `completed gates did not produce a candidate checkpoint`. After the repair it must finish a grounded independent review, retain no diff or deliverable, and expose no candidate authority. Replay the actual terminal state and denied controls:

```sh
python3 docs/testing/reproducers/rsi-inbox/check_no_patch.py --cli /home/doll/Peritus-RSI-2026-10-12/native/bin/peritus --endpoint FIXTURE_SOCKET --run 9e5489d8b5a3b89cc2e4a7dd2844f209 --output /tmp/peritus-rsi-no-patch-observations.json
```

For collection feedback, preserve that actual pre-fix failed run and its governing control store. In two fresh disposable copies, remove only the target inbox's already collected suggestion. Keep the source evaluation reservation in one copy, and remove it in the other. Start actual daemons and query the target inbox: the reserved evaluation must collect zero suggestions; the identical failed record without an evaluation reservation must collect one failure suggestion. These copied-state operations introduce no model calls or fabricated terminal receipts. The executed paired replay, copied config/storage, actual output, and owned process logs are under `native/collector-replay/{reserved,ordinary}`; its launcher is `native/collector_replay.py`.
