# Native Rust plugins and workflow modules

`peritus-modules` owns validated definitions, immutable packages, per-user enablement,
market discovery/installation, and existing declarative stage checkpoints. Native
Rust feature code runs through the product runner's generic adapter to
`peritus-plugin-host`; this crate contains no Deep Research implementation.

The official market is https://github.com/Corvidae-Coding-Projects/Peritus-Module-Market.
Open `/modules` in the TUI or **Modules** in the GUI. Refresh, install, explicitly enable,
and select Use module. The executable supplies its own action menus. Native packages
include a `[code]` ABI/target artifact/tool declaration instead of roles/stages. The
market contains the Rust SDK, full Deep Research source, a native code template, and an
additive source catalog for modules that are built locally rather than committed as executables.
The initial prebuilt research artifact and the first source-build recipe target Linux x86-64
GNU; other platforms need native artifacts or a separately established recipe.

Installation runs no code, binds all declared resources by digest, and starts disabled.
Updates also start disabled. Native code executes as the current OS account; agent
requests borrow only declared host services under live permissions. This mediation
is not an OS sandbox. Reinstalling an exact package can repair corrupt installed bytes
while retaining them for diagnosis. Removing a selection keeps immutable history.

```sh
peritus modules refresh
peritus modules market
peritus modules install-market peritus.deep-research EXACT_DIGEST
peritus modules build-install-market example.native-module EXACT_SOURCE_DIGEST
peritus modules enable peritus.deep-research
peritus modules show peritus.deep-research
```

Source refresh is inert. An explicit source build revalidates the pinned clean commit, source
tree, lockfile, toolchain and target; prepares locked public dependencies without credentials;
then compiles offline in a bounded Linux scope. Only a fully validated package is atomically
installed outside Git, and it remains disabled until the user enables it. Failure, cancellation
or host loss never replaces a prior installed selection. Build records and logs are retained for
inspection and explicit retry.

```text
/modules
/module peritus.deep-research YOUR QUESTION
/module peritus.deep-research plan
/module peritus.deep-research start EXACT_PLAN_DIGEST
/module peritus.deep-research status
/module peritus.deep-research show
/module peritus.deep-research resume
```

Stop uses normal run controls. Native plugins own their checkpoints and resume rules.
Deep Research waits for explicit Start, preserves completed stages, rejects stale plan
approval, and supplies source/report paging and an explicit Use in Design attachment.
Peritus supplies exact versioned attachment reads without inflating durable records.

## Existing declarative workflow format

Source brief and local data-only packages retain their prior contract and identity.
These use host-interpreted roles/stages with read-only tools. They do not implement
new native functions. Their earlier usage and minimal manifest follow.

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
→ final fact checking as a declarative baseline. The installed native **Deep Research**
plugin implements clarification, explicit Start, a structured source/admission ledger,
exact-report checking and Design reuse in its own Rust crate. The broader
[research roadmap](../../../.design/deep-research.md) retains richer source browsing,
PDF extraction and sustained-duration campaigns. AI source assessment does not establish
infallible factual truth.

See [AGENTS.md](AGENTS.md), [foundation design](../../../.design/module-foundation.md),
and [real-interface evidence](../../../docs/testing/module-foundation-real-2026-10-17.md).
