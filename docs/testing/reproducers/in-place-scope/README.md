# In-place directory tracking: real replay

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

Twenty journeys include nonempty brownfield scopes, create/modify/delete, empty and absent
directories becoming populated, legacy v1 journals, overlapping declarations, directory/file
replacement, protected paths and implicit cache/link exclusion, varied names (seed 260213),
post-admission growth beyond 4096 children and 64 nested levels, and actual Unix socket/FIFO
lifecycles and file/directory replacement by external symlinks. Socket fixtures bind relative
paths inside copied workspaces, remain alive during inspection, and stop on EOF. Old-atime
canaries with matching child names ensure symlink targets are never read or traversed.
Initially explicit symlinks remain rejected, and mixed opaque parent inventories stay usable. An explicitly requested
cache file remains an allowed exact read; a parent-directory declaration does not read caches.

The operator saves concrete inputs, expected outcomes, raw operations and results under
`--output/runs/`. Before-effect admission bounds may reject an overly broad new declaration;
they are not reapplied to invalidate already admitted work after population.

The socket/FIFO scenarios require Unix filesystem support (AF_UNIX and mkfifo); this retained
replay was exercised on Linux. Each consumer/file operation has a five-second bound.
