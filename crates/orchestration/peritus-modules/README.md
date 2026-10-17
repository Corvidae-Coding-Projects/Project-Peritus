# Workflow modules

`peritus-modules` owns declarative module definitions, immutable installed packages,
per-user enablement, and durable stage checkpoints. The product runner executes roles
through Peritus's existing provider, permission, trace, and cancellation boundaries.

Both bundled and separately installed modules use this contract. Installed modules
start disabled, and updating a package disables it again. Reinstalling the exact
original package repairs damaged installed bytes, preserves the damaged copy in a
quarantine directory, and requires explicit enablement again. Enablement grants no tool
permissions. A module cannot install executables, add tools, run shell commands, or
widen the host's read/network permissions. Process/Wasm plugins in `crates/extensions`
are a separate API and are not automatically loaded by these commands.

```sh
peritus modules list
peritus modules show peritus.source-brief
peritus modules install /path/to/module-directory
peritus modules enable local.example
peritus modules disable local.example
peritus modules remove local.example
```

In either the terminal or browser conversation:

```text
/modules
/module peritus.source-brief Your research question
/module status
/module show
/module show 32768
/module resume
```

Stop uses the normal run controls. Completed stages are retained; resume continues
the original task with the same package digest. Changing a package or disabling it
prevents further execution. Previously committed outputs remain inspectable. Removal
unregisters the package but retains immutable bytes and job history. To resume an old
version after an update, reinstall and enable that exact package. New user input
pauses a superseded stage; resume keeps the original task, while a new `/module ID
NEW TASK` starts a separate job. Do not treat a partial output as a finished report.

A minimal installable `module.toml`:

```toml
schema = 1
id = "local.example"
version = "1.0.0"
name = "Example"
description = "Explain a user question using official sources"
resources = ["instructions.txt"]

[[roles]]
id = "reader"
model = "writer" # existing writer or reviewer provider selection
instructions = "Read instructions.txt with module_resource_read, then answer the question."
tools = ["web_open", "web_read", "web_find"]

[[stages]]
id = "read"
role = "reader"
```

Only declared regular resource files are copied; symlinks, traversal, unknown fields,
invalid role/model references and forward/cyclic dependencies are rejected. Module
IDs `show`, `status` and `resume` are reserved. Tool names are checked against the
product's read-only catalog before inference, and enforced on every tool call.

Multiple stages declare `inputs = ["earlier-stage-id"]`. Every stage receives separate
working memory and only its declared predecessor outputs. `module_output_read`
provides exact paged continuation; `module_resource_read` reads declared package data.
The host binds output digests, stage order, original task, and package identity to an
exclusively owned job. It publishes complete artifact bytes before advancing the
small checkpoint, and checks integrity on reopening and reading.

The bundled **Source brief** exercises gathering → skeptical assessment → synthesis
→ final fact checking. It is a working baseline workflow. The broader [Deep Research
design](../../../.design/deep-research.md), including clarification, explicit Start,
a structured evidence ledger, revision-bound final approval, and report integration,
is a separate feature. Prompted skepticism does not establish factual truth.

See [AGENTS.md](AGENTS.md), [foundation design](../../../.design/module-foundation.md),
and [real-interface evidence](../../../docs/testing/module-foundation-real-2026-10-17.md).
