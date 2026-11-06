# Mutation testing

Use the real public models as the oracle for compiler-generated faults in production Rust:

```sh
python3 discovery/mutation/run.py inventory --output /tmp/peritus-mutants-inventory
python3 discovery/mutation/run.py run --output /tmp/peritus-mutants-run
```

The inventory includes every active root workspace package. `mutants.json` contains every selected mutation and its exact diff. The runner selects production source under `crates/` and `xtask/src/`; it does not mutate discovery oracles. An explicit `--file` replaces those defaults rather than broadening them, and the controller rejects candidates outside requested package, file, or regex filters. There are no function/file exceptions suppressing production findings. Platform `cfg`, Cargo features, generated code and macro expansion still limit what the current compiler can execute. Inventory is not proof of behavioral coverage.

The runner copies tracked and nonignored untracked source into a fresh directory **outside the checkout**, including current edits and deletions. Ignored files stay ignored. Existing release manifests, lockfile and source files in the original checkout are never edited. Only the disposable workspace gains three optional discovery members: the existing public property library, policy properties, and the mutation test bridge. Existing external lockfile entries must remain unchanged when those pinned dependencies are resolved. The source manifest records input hashes. A local shared Git clone gives the disposable tree its own index and references for source fingerprints; original repository metadata is not changed.

`cargo-mutants` 27.1.0's own baseline uses the packages being mutated, which can mean **zero production tests** here. The runner therefore builds and executes a separate required baseline against each selected production package's own tests plus the actual discovery packages, validates an execution marker for every selected public model, and only then runs mutants with `--baseline skip`. The skip refers to that redundant production baseline; it does not skip the controller baseline. For the mutant phase, the runner inserts the selected production packages into the disposable config's `test_package` list ahead of the discovery packages. Each candidate therefore executes its owner's focused tests as well as the public models and established debugger, evaluation, evolution, and telemetry tests. Direct unstaged `cargo mutants` cannot resolve the bridge package and fails its unmutated build rather than accepting an empty suite. Both package-selection mechanisms in the config are deliberate: cargo-mutants merely warns and ignores unknown `test_package` names, while the additional Cargo `--package` flags make missing discovery packages a hard baseline error.

A diagnostic invariant additionally drives actual codec rejection paths and requires distinguishable, meaningful human errors with their reported byte offsets; it does not copy exact message wording. It covers framing, malformed primitives, UTF-8, resource limits and protocol-domain errors.

The bridge reuses all 42 independent public model checkers and all fixed examples, plus generated operation histories (64 per model by default, fixed seed 7319). Policy test cases retain their existing 1,024-case configurations. Models call the actual production APIs; returned values and persistent effects are compared to their existing independent contracts. Tests run serially to preserve fixture isolation. On a mismatch the controller retains exact case JSON under `failures/`, with the mutant diff and detailed Cargo log under `mutants.out/`.

AgentDriver crash-cut checks launch the dedicated
`peritus-mutation-agent-driver-child` binary. They never spawn the Cargo test
harness as if it were the ordinary praxis executable; a focused regression
requires the exact provider-intent cut to be reached and recovered.

For a bounded or distributed campaign:

```sh
python3 discovery/mutation/run.py run --output /tmp/peritus-mutants-codec \
  --package peritus-codec --cases 256 --steps 128 --jobs 1 --build-jobs 1
python3 discovery/mutation/run.py run --output /tmp/peritus-mutants-shard-0 \
  --shard 0/64 --cases 64 --jobs 1 --build-jobs 1
```

Shard indices are zero-based: run `0/64` through `63/64` for all 64 shards.

`--package`, `--file`, and `--target` may repeat; `--regex` filters mutation names. `--regex` uses the common Python/Rust regex subset. The controller rejects an inventory containing candidates outside that selection (cargo-mutants 27.1.0 can emit unrelated struct-field deletions for a workspace regex); narrow with `--package`/`--file` or omit the regex for the full campaign. `--target codec` limits the public model campaign for focused calibration and is recorded in the request and baseline receipt; default campaigns run all models. Policy tests remain enabled. Never interpret a focused target run as a whole-codebase run. Multiple workers copy the built baseline; the default single worker mutates only the disposable source, restoring each candidate afterward. Prefer separate shards on memory-constrained hosts. The overall mutation search budget defaults to one hour (`--campaign-seconds 3600`); use a larger budget or additional shards for long campaigns. Exhausting that budget leaves an incomplete campaign with the unexecuted count recorded.

`report.json` retains cargo-mutants's detailed outcomes and nonzero exit status, separating caught, missed, unviable and timeout results. Missing or partial outcomes, an unexpected test-package command, or a surviving mutant that skipped a selected model produce an incomplete campaign instead of success. The required baseline and individual compiler/test commands have bounded budgets; expiration terminates their entire process group and retains the logs. A surviving mutant is a coverage lead, not automatically a defect: reproduce it, decide whether it is semantically equivalent or unreachable under the actual contract, then strengthen the checker where appropriate. Unviable mutations and timeouts never count as caught. Crashes from deliberate mutations are expected calibration artifacts; replay a suspected product defect against unmutated source before attributing it to Peritus.

The public library models and policy properties do not replace the separate owner-mounted daemon, private request/read/tool-plan, CLI/launcher, installer, browser, native Windows/macOS or authenticated-provider journeys. Use the umbrella discovery controller and coverage map to see those boundaries. No finite mutation campaign establishes that every execution is correct.

Per-mutant test budgets default to three times the observed baseline test execution
time (excluding compilation), with a 60-second minimum. `--timeout` supplies an explicit
override. `test-budget.json` records the calculation so a slow cryptographic campaign
is not mistakenly judged against a fixed short timeout.
