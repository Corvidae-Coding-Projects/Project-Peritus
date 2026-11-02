# Mutation and coverage-guided testing

These optional tools check the current source through actual Peritus libraries and the
existing property models. They stay outside the product workspace and release graph.
The root compiler remains Rust 1.97.1. Tool versions are in `tools.toml`.

## Engines and checks

- **cargo-mutants** changes production Rust code in a disposable source copy and runs
  independent property checks. A mandatory, verified baseline runs before mutation.
  The runner preserves each mutant's diff, logs, classification, and failing inputs.
- **cargo-fuzz / libFuzzer** uses instrumented coverage and comparison feedback.
  The default campaign enables AddressSanitizer, rebuilt Rust standard-library checks,
  overflow/debug assertions, careful mode, comparison feedback, and value profiling.
- **LibAFL** provides an alternate search engine for the same target implementations.
  Its flags, runtime dependency resolution, and result accounting are documented in
  `fuzz-libafl/README.md`. An expired watchdog is incomplete, and any crash artifact
  fails the run even if the engine returns exit code zero.
- Existing **proptest campaigns** retain stateful persistence, authority, recovery,
  daemon steering, and policy oracles that fast in-process fuzzing cannot replace.

The Rust Fuzz Book recommends cargo-fuzz:
https://rust-fuzz.github.io/book/cargo-fuzz.html

The alternate runtime's interface is documented upstream:
https://docs.rs/libafl_libfuzzer/0.16.1/libafl_libfuzzer/

## Setup and broad baseline

Run from the repository root. Python 3.11+, rustup, and Clang's C/C++ compilers are required.
Every output directory must be new. Keep evidence outside the source checkout.
Fuzz builds explicitly use `CC=clang` and `CXX=clang++`. Mixing GCC-instrumented native
dependencies with LLVM's ASan runtime caused a pre-input SQLite ODR failure in the
initial qualification; the compiler choice fixes that setup problem without disabling
ODR or memory checks. Ordinary product and mutation builds keep their normal compiler.
The campaign builds with cargo-fuzz and executes the resulting, hash-recorded binary
directly. Upstream cargo-fuzz 0.13.2's `run` command unconditionally disables ASan's
ODR check; direct execution lets this runner retain `detect_odr_violation=2`.

```sh
python3 discovery/checks.py setup --output /tmp/peritus-tools-1
python3 discovery/coverage.py --output /tmp/peritus-package-map-1.json
python3 discovery/mutation/run.py inventory --output /tmp/peritus-mutants-inventory-1
python3 discovery/checks.py baseline --output /tmp/peritus-baseline-1 \
  --target-dir target/praxis --cases 256 --jobs 1
```

The baseline runs current-host root library/integration tests, all 42 public campaigns,
all six owner campaigns, and the separate policy suites. Root tests include analysis,
telemetry, and context-flow checks. Search receipts record actual inputs and execution;
the baseline also tests evolution's `qualification` feature and compiles the agent without
its protocol bridge and the system-package clients. Test-only fake-provider/store features
do not establish real-provider or credential-store behavior. Source associations in the
package map do not count as tests. Production dependency
versions and user installations are preserved.

## Fuzz all targets

```sh
python3 discovery/checks.py fuzz --output /tmp/peritus-fuzz-1 --seconds 60
python3 discovery/checks.py fuzz --engine libafl --output /tmp/peritus-libafl-1 \
  --runs 500 --seconds 120
```

The 12 targets cover framed bytes and primitive encoding, SSE and NDJSON chunking,
bounded/canonical JSON, schema acceptance and rejection, provider JSON repair,
workspace paths, exact permission sets, context graphs, and statistical accounting.
Inputs include raw malformed bytes, constructive valid cases, boundary violations,
and independent oracles. Each target starts from small checked-in seeds and retains
its coverage corpus and crashes in the evidence directory. Limits deliberately bound
input size and graph/operation counts; they are visible in `fuzz/src/lib.rs`.

`--seconds` is a search budget per target, excluding compilation. `--jobs` caps Cargo
compilation; default one avoids competing with active product work. Use
`--target sse` to focus a target. `--corpus /path/to/corpus` resumes that target's saved
corpus. For `--target all`, the supplied directory must contain one corpus directory
named for each target. `--sanitizer thread`, `memory`, or `leak` selects another
instrumentation mode when supported by the pinned compiler and host. Those modes
require their own successful build/run; an ASan pass does not establish their results.
`--extra-traces` also instruments division and index expressions, with a separate rebuild.
LibAFL instead completes `--runs` fuzzer stages under a bounded watchdog. The measured
500-stage default fits the slowest current target on the qualification host; request a
larger value explicitly for deeper campaigns. These stages are
not exact invocation counts; its monitor counters are retained as observed lower bounds.
LibAFL has no supported deterministic seed flag. Its exact corpus and crash inputs
remain replayable through the primary engine. Separate default build directories prevent
one backend from accidentally executing the other's same-named binaries.

The runner binds evidence to current source bytes. A source edit during the campaign
makes its final status incomplete. Build failure, missing execution statistics, timeout,
OOM, or crash cannot become a successful search result.
Run `python3 -m unittest discover -s discovery -p 'test_*.py'` after changing a campaign
controller. These tests ensure a retained failure cannot be hidden by a simultaneous
incomplete target, LibAFL monitor counters are understood before a target can pass, and
mutation builds do not depend on ambient compiler caches.

AddressSanitizer runs default to `detect_leaks=0` because LeakSanitizer cannot operate
under this managed sandbox's ptrace restrictions. `--detect-leaks` enables it on a
compatible host; failure to run there remains incomplete. Heap/stack bounds, use-after-free,
strict ODR checks, debug assertions, and careful-mode checks remain enabled by default.

## Mutation campaigns

```sh
# Run every generated mutation against the default model/test packages.
python3 discovery/mutation/run.py run --output /tmp/peritus-mutants-full-1

# Distribute that workload across 64 disjoint shards, retaining each result.
python3 discovery/mutation/run.py run --output /tmp/peritus-mutants-shard-0 --shard 0/64

# Investigate a particular mechanism with all default oracles still enabled.
python3 discovery/mutation/run.py run --output /tmp/peritus-codec-mutants-1 \
  --package peritus-codec --file 'crates/foundation/peritus-codec/src/**'
```

Shard indices are zero-based: run `0/64` through `63/64` to complete this sweep.
Missed-mutation lists can be examined directly in retained cargo-mutants output;
never infer completion
from one shard. The initial host/default-feature inventory has 81 packages and 88,709
candidates; rerun inventory after changes. A full sweep is a substantial workload.

Use the wrapper rather than bare `cargo mutants`: it adds the discovery test packages
only inside the disposable workspace, inserts every production package represented by
the selected mutation inventory into that campaign's test list, reconciles the lockfile
without changing existing external dependency entries, and requires proof that both the
owner tests and selected models executed.
Explicit `--file` filters replace the default production globs, and the inventory fails
closed if cargo-mutants emits a package, file, or regex candidate outside the request.
The checked-in cargo-mutants configuration fails safely in an unstaged root workspace.
See `mutation/README.md` for target filters and outcome semantics. Surviving mutants
identify missing reachability, assertions, or equivalent behavior to investigate;
they are not automatically production bugs. Uncompilable mutants are not caught tests.

## Reproduce and reduce failures

```sh
python3 discovery/checks.py replay --target sse --case /path/to/crash-input \
  --output /tmp/peritus-crash-replay-1
python3 discovery/checks.py minimize --target sse --case /path/to/crash-input \
  --output /tmp/peritus-crash-minimize-1 --seconds 60
```

Confirm the oracle and legal input contract before changing production. Preserve the
concrete minimized bytes and initial failing output, make the smallest repair, and
replay through the real public interface. Security-sensitive reproductions remain local
pending private triage. A hypothetical mutation is calibration, not a discovered bug.

For source coverage, replay the retained corpus with the pinned toolchain's
`cargo fuzz coverage` command. Coverage mode uses a separate instrumentation build;
its output cannot be inferred from fuzz throughput or the package inventory. For example:

```sh
CC=clang CXX=clang++ cargo +nightly-2026-08-09 fuzz coverage sse --fuzz-dir discovery/fuzz \
  --target-dir target/fuzz-coverage /path/to/saved/sse/corpus
```

## Scope accounting

`coverage.py` fails if the root workspace and its ownership map drift. It reports every
active package, current consumer source associations, private owner campaigns, and
unexecuted platform boundaries. Campaign receipts separately record the actual source,
commands, budgets, cases, and results.

Finite tests cannot establish correctness for every input or schedule. Native macOS and
Windows behavior needs native runs. Browser/desktop interactions, account-authenticated
providers, installers, process supervisors, and effects outside the instrumented process
still need their real-interface workflows. ASan careful mode instruments Rust std; it
does not automatically instrument every C dependency or child process, or prove freedom
from data races. These remain explicit gaps rather than implicit passes.
