---
name: real-testing
description: Verify software changes by building and running the actual program, exercising its real interfaces, systematically generating varied inputs and action sequences, checking independent requirements and invariants, and preserving reproducible failures for regression replay. Use after implementing a feature or bug fix, when asked to test that software actually works or discover missed bugs, or when replacing routine unit-test generation with direct behavioral verification. Support CLI programs, services, web and desktop interfaces, libraries, and background jobs.
---

# Real Testing

Build it. Run it. Operate the changed feature. Explore how it fails. Preserve what you discover.

Treat successful compilation, startup, an HTTP 200, a success message, or a green unit-test suite as intermediate evidence. Establish completion by exercising the requested behavior and observing the required output or state change.

Combine direct feature acceptance with systematic exploration. Use representative workflows to establish the promised behavior, then use generated cases to search for failures beyond those examples.

## 1. Define the observable contract

- Read the user's requirement, relevant project instructions, and the changed code or diff. Identify each new or changed behavior and the interface through which its caller or user reaches it.
- Write a compact set of scenarios with the action or input and the expected observable result **before** running them. Derive expectations from the requirement, documented contract, a known real bug, or a trusted independent reference. Never adopt the program's output as the expected answer simply because it produced it.
- Include the meaningful ordinary workflow and the boundary, invalid-input, or recovery scenario most relevant to the change. Add cases only for distinct failure modes; apply no fixed case count or coverage quota.
- Choose inputs that expose the changed semantics. Use distinct values where order, identity, or routing matters; avoid symmetric examples that conceal mistakes.
- Include the immediately affected existing workflow when the change could break it. Define tolerances or acceptable outcomes in advance for approximate or nondeterministic behavior.
- Resolve routine ambiguity from project context and state the assumption. Ask only when missing requirements materially prevent deciding what counts as correct; continue independent checks meanwhile.

## 2. Build and launch the current program

- Discover the actual build, setup, and run commands from the project. Use the supported toolchain, required configuration, and ordinary runtime dependencies.
- Compile the changed application or affected component. For interpreted software, perform the project's relevant syntax or import check and then run the actual entry point; do not invent a compilation step.
- Verify that the executable, bundle, container, or installed package being exercised contains the current change. Rebuild and restart after a fix; avoid stale artifacts, cached bundles, and unrelated running instances.
- Launch the application and its required services with isolated test data. Use real local databases, files, queues, and service implementations where needed. Observe readiness and startup errors; do not treat starting a process as evidence that it is ready.
- Keep long-running processes manageable through available session or process tools. Record which processes and temporary resources you create.
- Continue routine local setup and reversible checks within the user's authorization. Use test accounts or sandboxes for external effects. Respect the session's tool, credential, and action permissions; the skill grants no extra authority.
- When an actual required dependency or interaction tool is unavailable, identify the missing condition and which behaviors it blocks. Test the remaining reachable behavior. Never silently replace the missing path with a mock and mark it verified.

## 3. Operate the feature through its real interface

Choose the interface the change promises to support:

| Program or change | Required exercise |
| --- | --- |
| CLI | Invoke the built command with realistic arguments, input, and files; inspect output, exit status, and produced artifacts. |
| API or service | Run the service and send requests through its real network or IPC interface; inspect response contents and downstream effects. |
| Web or desktop UI | Open the running application using available permitted interaction tools; perform the actual user actions and inspect the displayed result and saved state. |
| Library | Build a minimal real consumer using the public API; run it against the changed library and inspect the returned values or effects. |
| Worker, scheduler, or pipeline | Submit actual work through the supported entry point; observe processing and the resulting destination or artifact. |

- Exercise the complete changed workflow. Include the real caller, wiring, runtime configuration, and dependencies needed for the promised behavior.
- For UI changes, operate the changed controls. A direct backend request can supplement that check but cannot establish that the UI interaction works.
- For a save, create, edit, delete, upload, or export operation, read back the resulting data through the normal consumer path. Inspect content as well as existence; independently parse or open a generated artifact when relevant.
- For promised persistence, close and reopen the program or reconnect a fresh client, then verify the state again. For background work, wait for actual completion within a sensible deadline and inspect its output.
- Check the relevant failure behavior too: rejection, error presentation, recovery, or absence of unintended state changes, according to the contract.
- Inspect runtime logs, stderr, and browser or application errors relevant to the exercised path. Investigate evidence of hidden failure even when the surface reports success.
- Use short commands, existing clients, or a small driver when helpful. Make the driver operate the real program and compare results with independently established expectations. Keep any reference model limited to the observable contract and structurally different from the implementation; never copy the production algorithm into the checker.
- Establish the origin of evidence: record the executable or build, runtime instance or endpoint, configuration, and actual commands or actions that produced it. Capture screenshots or recordings from that application session. Inspect the driver and fixtures for fabricated pages, hardcoded answers, or substituted feature implementations; distinguish ordinary test data from a counterfeit application.
- Reproduce important or surprising findings through a fresh process, client, or clean session. When independent agents are available and useful, ask one to verify the raw reproduction against the requirement in a separate context; provide the inputs and artifacts without the first agent's diagnosis. Require execution evidence rather than agreement between explanations.

## 4. Explore systematically

- Identify the changed feature's failure-prone dimensions from its contract, implementation, and relevant available bug history. Consider values, sizes, ordering, identities, encodings, lifecycle transitions, retries, persistence, and interaction with adjacent features where applicable.
- Build or reuse a small input or action-sequence generator that drives the same real interface as the direct checks. Prefer structured, valid inputs and reachable states that exercise substantive behavior. Include targeted invalid inputs for rejection and recovery; avoid spending the whole exploration budget on random bytes or repeated early rejection paths.
- Vary dimensions individually, then combine those that can interact. Include distinct identities and values, empty and boundary-sized collections, and repeated or reordered actions when the contract permits them. For stateful software, generate sequences such as create, update, read, restart, retry, and delete with prerequisites satisfied. Explore concurrency only when it is part of the affected behavior.
- Define the oracle before evaluating cases. Check documented invariants, independently computed expected state, a trusted reference implementation, or a metamorphic relation justified by the contract. For example, check that restarting preserves committed data only when persistence is promised, or that an invalid operation leaves state unchanged only when the contract requires that. Do not invent properties just because they are easy to assert.
- Keep the checker independent: obtain expectations from requirements and explicit state transitions rather than the implementation's current outputs. If the same agent writes the driver, require that separation in the code and inspect it. Use a separate context for oracle design when available and worthwhile, providing the contract rather than the implementation's proposed answer.
- Choose a bounded run budget suited to the changed behavior and available runtime. Record the seed, generator version or command, configuration, number of executed cases or sequences, and dimensions explored. Inspect representative generated cases and actions actually executed; naming a technique or calling a generator is not evidence that substantive exploration occurred.
- Execute generated cases against the current program and inspect the actual outputs and downstream state. For persistent or stateful behavior, interleave fresh-process or fresh-client readbacks when relevant. Investigate invariant violations, crashes, hangs, and unexplained runtime errors through the real failing path.
- Confirm that a suspected failure uses a legal scenario and a correct oracle. Rerun the concrete inputs against the real program; distinguish generator or checker defects from application defects. Minimize the input or action sequence while retaining the same failure, prerequisites, and interface.
- For nondeterministic or timing-dependent failures, preserve the relevant schedule or timing, environment, and observed reproduction frequency. Report the uncertainty; a single successful rerun does not erase an observed failure.
- Use escaped bugs and unexplored behavior as feedback: add the missing input dimension, state transition, or justified property to the generator so it can discover related failures. Avoid only hardcoding the first failing example.

## 5. Fix and repeat

- Compare actual observations with the predefined expectations. Classify each scenario as **PASS**, **FAIL**, or **BLOCKED**.
- On a failure, capture the input or actions and the observed mismatch. Inspect the real failing path, fix the cause within the task's scope, rebuild or restart, and repeat the scenario plus directly affected checks.
- Keep the expected behavior stable. Correct an erroneous expectation only with a stated reason grounded in the requirement or an independent source; never loosen it merely to get a pass.
- Record an initially observed failure as well as its final result when it explains the fix. Where practical for a bug fix, demonstrate that the original program exhibits the reported failure and the changed program resolves it.
- Stop when the final changed program satisfies the direct scenarios, saved regressions, and bounded generated checks, with no observed relevant failure unresolved. Report an unresolved failure, blocker, or exploration gap plainly; none counts as a pass. Report the budget and scope rather than claiming exhaustive coverage.

## 6. Preserve discovered failures

- Save each confirmed failure as a small replayable case in the project's existing reproduction or regression location; otherwise use a clearly named project-local location. Keep the concrete input or action sequence, prerequisites, independently justified expected result, observed failing result, and exact replay command.
- Retain the seed and generator settings as supporting evidence, but also save the concrete failing case: a seed alone can stop reproducing when a generator changes. Preserve required fixture data and include clean-state setup so prior runs do not contaminate replay. Redact secrets and avoid retaining unrelated personal data.
- Make replay operate the real application, executable, service, UI, or public library interface. Ensure the replay detects the original mismatch and reports failure if it returns. Retain the case after the fix, verify it against the final version, and keep it separate from disposable exploration output.
- Keep the generator or a reproducible generation command when it is useful for future changes. Replay saved failures and explore fresh generated cases on subsequent relevant invocations; do not replace exploration with repeatedly running only the same fixed examples.
- Preserve only cases and resources that demonstrate a distinct failure or support useful generation. Avoid accumulating large successful-run transcripts, redundant cases, or artificial unit-test suites.

## 7. Report observed evidence and clean up

Give a brief result that contains:

- The build and launch commands, tested version or artifact, and relevant environment.
- The actual actions or inputs, expected behavior, observed results, and scenario status. Use a compact table when multiple scenarios benefit from comparison.
- The generated exploration budget and actual case count, input dimensions or action sequences exercised, independent properties checked, and any remaining gaps.
- The location and replay command for retained failures, and confirmation that their original mismatch is resolved on the final program.
- Any fix made, any remaining failure, and the exact boundary of blocked or unexercised behavior.
- Enough detail to repeat the important workflow without exposing secrets or personal data.

Stop only processes you started and remove disposable test data or resources you created. Preserve the saved failure cases, required replay fixtures, useful generator, and supporting failure evidence.

Claim completion only for behavior you actually exercised successfully on the final version. Say when validation was partial; do not turn a narrow successful check into a claim about the whole program.

## Keep verification focused

- Use direct operation and bounded systematic exploration as the default completion checks. Do not generate unit tests, mocks, snapshots, assertion counts, or coverage targets merely to satisfy a testing ritual.
- Run an existing automated check when project instructions require it or it provides relevant additional evidence. Keep saved real failure replays within the verification task; delete existing tests or redesign CI only when the task authorizes those changes.
- Automate a real workflow when repeatability is useful, while keeping the actual interfaces and dependencies in the exercised path. Judge the check by the behavior it distinguishes and the evidence it provides.
