# Canonical automatic skill bundling — real testing

Requested behavior: all bundled guidance lives under repository `skills/`; adding a package and
its resources requires no per-skill Rust registration, tool-schema changes, or duplicated text.

The previous code kept real-testing and role instructions in crate-local Markdown and manually
mirrored Rust strings. Some mirrors had already diverged. Rust reliability also had a hand-written
resource whitelist. The migration preserves the effective compiled instruction bodies, license,
and every existing Rust reliability resource, then removes those duplicate sources and registries.

## Expected observable behavior

These expectations were established before the replay:

- Discover all canonical packages by directory identity and `SKILL.md`, and list descriptions and
  all original resource paths. A new package, nested file, or arbitrary extension needs no Rust edit.
- Reconstruct every source file byte-for-byte through actual paginated readers. Validate page
  budgets, original sizes, stable SHA-256 identities, progress, UTF-8 boundaries, binary encoding,
  and empty files/EOF against source bytes rather than implementation-derived answers.
- Pick up package addition/removal and resource addition/update/deletion through ordinary Cargo
  rebuilding. Already compiled resources remain readable when their source directory is absent.
- Reject unknown names, traversal and absolute paths, invalid offsets and invalid page budgets.
  Do not execute a helper merely because it is bundled or read.
- Keep current real-testing policy and Rust activation behavior, permissions, and existing skill
  names. Discover detailed guidance on demand rather than injecting all documents into every prompt.

## Build and replay

The affected product-runner crate is built with the supported toolchain, `--locked`, and two Cargo
jobs. The independent native consumer is built offline against the changed local library and the
workspace dependency versions. It operates the same catalog/page readers as model tool delivery.

```sh
cargo build --locked -p peritus-product-runner -j 2
python3 docs/testing/reproducers/bundled-skills/replay.py
```

Raw evidence: `/home/doll/.cache/peritus-skills-20261020/`. The retained
[reproducer](reproducers/bundled-skills/README.md) specifies seed `20261020`, original-byte oracle,
exact commands, fixture ownership, and evidence boundaries. No unit, formal-verification, or
repository qualification suite is used. Provider inference and UI behavior are outside this replay.

## Observed results

PASS: all eight canonical skills and all 26 original resource files round-trip byte-for-byte.
PASS: adding two packages without any Rust/schema edits discovers ten skills and 34 resources.
Folded YAML descriptions, CRLF documents and a plain Markdown heading fallback are observed.
PASS: resource update/deletion and a new nested arbitrary-extension binary are detected by rebuilding.
PASS: the compiled consumer still reads the same 34 resources through fresh processes after the
fixture source package is moved outside the canonical tree.
PASS: rebuilding after fixture removal returns to exactly eight skills and 26 resources.
PASS: unknown packages/files, traversal, absolute paths, invalid UTF-8 offsets, out-of-range offsets,
invalid page budgets, empty files and exact EOF behave as specified. No helper is executed.

The completed seed-20261020 campaign made 3,799 native consumer requests, including 3,781 exact
pages. Every reconstructed resource and catalog was independently checked for content, size,
page budget, progress and SHA-256. These counts measure this bounded replay, not exhaustive coverage.
A separate comparison against the original Git revision confirms the seven migrated active
instruction bodies, the original real-testing license and every Rust reliability file are unchanged.
