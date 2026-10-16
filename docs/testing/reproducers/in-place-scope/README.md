# In-place directory tracking: real replay

## Folder guide

Real-interface replay resources for in place scope; start with the local README/driver and its documented disposable-state prerequisites.

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |
| [consumer.rs](consumer.rs) | A command-line host of the actual public scoped-baseline API. |
| [replay.py](replay.py) | Replay actual Peritus directory tracking on archived copies, without unit tests. Requires Cargo, Python 3, git, and GNU patch. Outputs stay in the supplied disposable directory. |
| [runtime_setup.py](runtime_setup.py) | Create a fresh owned runtime fixture and copy actual installed binaries. |
| [runtime_replay.py](runtime_replay.py) | Operate real source edits and commands through the TUI/daemon/Codex while a bounded runtime writer remains active. |

This consumer calls the production `ScopedBaseline` API in fresh processes. The operator
archives actual Peritus source files, performs real filesystem and GNU patch operations, and
checks action-derived path sets, before/after content and unchanged source hashes. It uses no
mock provider, copied feature implementation or unit-test harness.

Create a disposable Cargo consumer outside the checkout with `consumer.rs` as `src/main.rs`:

```toml
[package]
name = "independent-peritus-scope-consumer"
version = "0.1.0"
edition = "2024"

[workspace]

[dependencies]
peritus-product-runner = { path = "/absolute/path/to/Project-Peritus/crates/app/peritus-product-runner" }
serde_json = "=1.0.149"
```

Build and operate it, supplying your actual checkout and disposable output paths:

```sh
cargo build --manifest-path /path/to/consumer/Cargo.toml --jobs 2 --target-dir /path/to/Project-Peritus/target
python3 docs/testing/reproducers/in-place-scope/replay.py \
  --consumer /path/to/Project-Peritus/target/debug/independent-peritus-scope-consumer \
  --output /path/to/disposable-scope-replay
```

Twenty-four journeys include nonempty brownfield scopes, create/modify/delete, empty and absent
directories becoming populated, legacy v1 journals, overlapping declarations, directory/file
replacement, protected paths and implicit cache/link exclusion, varied names (seed 260213),
post-admission growth beyond 4096 children and 64 nested levels, and actual Unix socket/FIFO
lifecycles and file/directory replacement by external symlinks. Socket fixtures bind relative
paths inside copied workspaces, remain alive during inspection, and stop on EOF. Old-atime
canaries with matching child names ensure symlink targets are never read or traversed.
Initially explicit symlinks remain rejected, and mixed opaque parent inventories stay usable. An explicitly requested
cache file remains an allowed exact read; a parent-directory declaration does not read caches.
Later child-directory declarations must preserve earlier file baselines and new children,
including growth beyond the original directory-admission bounds.
The runtime lifecycle journey performs forty additional fresh-process inspections while a
seeded writer creates/removes files and their parent directory. A separate source edit must
remain visible throughout, and the final inspection must return exactly that source change.

The operator saves concrete inputs, expected outcomes, raw operations and results under
`--output/runs/`. Scope inventories use an indexed transactional store and preserve earlier before-images.
Directory count, nesting depth, and journal size do not impose lifetime admission quotas.
The replay seeds concrete legacy v1 inputs before opening the new index; private/build/link
exclusions remain required.

The socket/FIFO scenarios require Unix filesystem support (AF_UNIX and mkfifo); this retained
replay was exercised on Linux. Each consumer/file operation has a five-second bound.

## Running services and the full product path

Run the following as the account whose real Codex credentials Peritus will use. The binaries
must be actual sibling `peritus`, `peritusd`, and `peritus-tui` executables. `FIXTURE` must be a
new disposable path. Python needs `pyte`; setup uses no provider mock or substitute daemon.

```sh
python3 runtime_setup.py --root "$FIXTURE" --binary-dir "$BINARY_DIR" --runtime-mib 1
python3 runtime_replay.py --root "$FIXTURE" --binary "$FIXTURE/bin/peritus" \
  --seconds 360 --stop-after-operations
```

Repeat in a fresh fixture with `--runtime-mib 16` at setup and `--writer-after-effect` at replay.
The first timing starts writes before enrollment; the second starts when the real command
creates its output. Both require the source edit, created output, and an actual Peritus
`python3 main.py` tool call returning exactly `SOURCE-REPAIRED` plus a newline, while the
writer is alive and its counter advances. The journal must contain the actual `.prescient`
parent declaration and its runtime-file descendant; scoping only the two output files cannot
pass. Screens, original trace frames, durable run state, binary hashes, and an
independent CLI readback remain under the fixture root. The script exits nonzero if its
operation oracle is not satisfied.

`--stop-after-operations` deliberately cancels the disposable run after this evidence is
observed. It establishes successful operations, not final candidate qualification under
continuously changing runtime data. Omit it to request a complete workflow; timeouts and
provider failures remain incomplete outcomes. Cleanup stops only this fixture's processes.

Directory declarations retain their descendants as comparison evidence. Automatic command
before-images cover exact file declarations, rather than every live database/log file found
under a directory. A later exact child declaration preserves the parent's original baseline;
direct file writes/patches still require their normal exact before-image checkpoint.
