# peritus-tools-fs

Production filesystem tools for Peritus's model-facing C4 boundary.

The canonical catalog contains `fs.discover`, `fs.metadata`, `fs.read`, `fs.search`, `fs.create`,
`fs.write`, `fs.remove`, `fs.replace`, and `fs.patch`. Schemas are built from the bounded C4 schema
model and descriptors carry exact B1 operation classes, risk sets, limits, replay semantics, and
unique implementation identities.

Reads use a C1 `ReadOnlyWorkspace`. Paths are `WorkspacePath` values; traversal is deterministic,
bounded, protected metadata is filtered, and symlinks and special nodes are rejected without being
followed. Text is returned as UTF-8 and binary data as explicit base64 with an exact source digest.
Literal search has independent depth, entry, per-file, aggregate-byte, and match limits.

Mutations never call ambient filesystem write APIs. Every create, write, remove, replacement, or
multi-file patch compiles to an inert canonical `PatchSet`. `FsDispatcher` is the sole adapter that
can pass it to `WorkspaceGateway::apply_patch`, and its only effect entry consumes a router-created
`AuthorizedInvocation`. The dispatcher compares the validated C4 caller/target/digest binding with
the C1 authorization binding before effect. Successful `MutationOutcome` remains available for a
separately authorized Git candidate operation.
