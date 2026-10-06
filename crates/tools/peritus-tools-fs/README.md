# peritus-tools-fs

Production filesystem tools for Peritus's model-facing C4 boundary.

The canonical catalog contains `fs.discover`, `fs.metadata`, `fs.read`, `fs.search`, `fs.create`,
`fs.write`, `fs.remove`, `fs.replace`, and `fs.patch`. Schemas are built from the bounded C4 schema
model and descriptors carry exact B1 operation classes, risk sets, limits, replay semantics, and
unique implementation identities.

Reads use a C1 `ReadOnlyWorkspace`. Paths are `WorkspacePath` values; traversal is deterministic,
bounded by caller policy, and protected metadata is filtered. Discovery and search retain usable
siblings alongside typed exclusions for unsupported native names, links and special nodes. Links
are never followed. Exclusions carry exact tagged native units as base64, escaped display text,
parent directory, depth and reason; their display text cannot be used as an authority path.
Exclusion-free observation digests and structured bytes remain exact; observations with exclusions
bind them under a separate version-two digest. Text is returned as UTF-8 and binary data as explicit
base64 with an exact source digest.
Literal search has independent depth, entry, per-file, aggregate-byte, and match limits.

Mutations never call ambient filesystem write APIs. Every create, write, remove, replacement, or
multi-file patch compiles to an inert canonical `PatchSet`. `FsDispatcher` is the sole adapter that
can pass it to `WorkspaceGateway::apply_patch`, and its only effect entry consumes a router-created
`AuthorizedInvocation`. The dispatcher compares the validated C4 caller/target/digest binding with
the C1 authorization binding before effect. Successful `MutationOutcome` remains available for a
separately authorized Git candidate operation.

Typed mutation inputs and their compiled patches have no inherited file-byte, aggregate-byte or
operation-count allowance. Mutation schemas omit the former `edits.maxItems` and preimage-size
maximum, so an exact authorized large-file deletion does not require transporting the file body.
The inline JSON transport/content-string bounds and inspection/search policies remain independent
contracts. Path schemas omit the former 4,096-byte maximum; the host-aware `WorkspacePath` authority
validator owns path safety. Existing field names and meanings are unchanged; descriptor digests bind the complete
current schemas and accepted outcomes retain their original exact evidence.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-tools-fs
```
