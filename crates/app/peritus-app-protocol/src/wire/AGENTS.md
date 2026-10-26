# crates/app/peritus-app-protocol/src/wire — navigation

## Responsibility

Canonical binary codecs and closed dispatcher for all A3 application families.

## Files in this folder

| File | Role |
| --- | --- |
| [artifact.rs](artifact.rs) | Canonical artifact-transfer value helpers. |
| [command.rs](command.rs) | Canonical exact command binding and final-result helpers. |
| [control.rs](control.rs) | Canonical schema-v1 application control family. |
| [daemon.rs](daemon.rs) | Canonical daemon status, heartbeat, and shutdown helpers. |
| [doctor.rs](doctor.rs) | Additive local diagnostics codecs with preallocation limits. |
| [error.rs](error.rs) | Canonical stable application error representation. |
| [event.rs](event.rs) | Canonical schema-v1 application event family. |
| [hello.rs](hello.rs) | Canonical client/server negotiation family codecs. |
| [improvements.rs](improvements.rs) | Bounded canonical candidate-inbox wire values. |
| [interaction.rs](interaction.rs) | Interaction and model-discovery codecs. |
| [mod.rs](mod.rs) | Canonical binary codecs and closed dispatcher for all A3 application families. |
| [primitive.rs](primitive.rs) | Canonical scalar and aggregate helpers shared by A3 wire families. |
| [product.rs](product.rs) | Canonical product-run request and observation encoding. |
| [prompt.rs](prompt.rs) | Canonical prompt binding, answer, and cancellation helpers. |
| [request.rs](request.rs) | Canonical schema-v1 application request family. |
| [request_inspection.rs](request_inspection.rs) | Bounded Workbench inspection request payload encodings. |
| [response.rs](response.rs) | Canonical schema-v1 application response family. |
| [subscription.rs](subscription.rs) | Canonical event-subscription value helpers. |
| [terminal.rs](terminal.rs) | Canonical terminal attachment value helpers. |
| [workbench.rs](workbench.rs) | Exact additive typed control codecs; no raw JSON or prompt macro crosses A3. |
| [workbench_brief.rs](workbench_brief.rs) | Bounded exact brief wire contract; no model suggestions can become confirmed fields. |
| [workbench_brief_pages.rs](workbench_brief_pages.rs) | Additive brief paging codecs; existing complete-brief bytes retain their schema. |
| [workbench_checkpoint_pages.rs](workbench_checkpoint_pages.rs) | Canonical page and continuation forms for complete checkpoint coverage. |
| [workbench_checkpoints.rs](workbench_checkpoints.rs) | Canonical bounded checkpoint and rewind forms. |
| [workbench_compaction.rs](workbench_compaction.rs) | Canonical content-free deterministic compaction request and preview forms. |
| [workbench_context.rs](workbench_context.rs) | Exact bounded content-free context wire forms. |
| [workbench_files.rs](workbench_files.rs) | Canonical typed file preview and selection codecs. Paths are inert descriptors. |
| [workbench_goal.rs](workbench_goal.rs) | Canonical goal definitions and projections. |
| [workbench_image_page.rs](workbench_image_page.rs) | Exact paged image metadata; row counts and labels are bounded before owned allocations. |
| [workbench_images.rs](workbench_images.rs) | Canonical image previews and scoped uploads; original media uses the existing chunk transfer. |
| [workbench_init.rs](workbench_init.rs) | Canonical bounded project-initialization discovery and proposal codec. |
| [workbench_init_artifacts.rs](workbench_init_artifacts.rs) | Additive artifact-backed initialization frames; legacy layouts remain unchanged. |
| [workbench_inputs.rs](workbench_inputs.rs) | Exact queue wire forms. All collection bounds are checked before allocation. |
| [workbench_launch.rs](workbench_launch.rs) | Canonical bounded launch, capture, result and artifact-feedback codecs. |
| [workbench_library.rs](workbench_library.rs) | Canonical conversation-library and fork codecs. |
| [workbench_memory.rs](workbench_memory.rs) | Canonical project-guidance queries, mutations, active records and content-free tombstones. |
| [workbench_permissions.rs](workbench_permissions.rs) | Canonical effective-permission projection codec. |
| [workbench_review.rs](workbench_review.rs) | Canonical structured-review codecs with bounded collection allocation. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [product/](product/AGENTS.md) | Implementation pieces for `product.rs`: Canonical product-run request and observation encoding. |
| [request/](request/AGENTS.md) | Implementation pieces for `request.rs`: Canonical schema-v1 application request family. |
| [workbench/](workbench/AGENTS.md) | Implementation pieces for `workbench.rs`: Exact additive typed control codecs; no raw JSON or prompt macro crosses A3. |
| [workbench_checkpoints/](workbench_checkpoints/AGENTS.md) | Implementation pieces for `workbench_checkpoints.rs`: Canonical bounded checkpoint and rewind forms. |
| [workbench_files/](workbench_files/AGENTS.md) | Implementation pieces for `workbench_files.rs`: Canonical typed file preview and selection codecs. Paths are inert descriptors. |
| [workbench_launch/](workbench_launch/AGENTS.md) | Implementation pieces for `workbench_launch.rs`: Canonical bounded launch, capture, result and artifact-feedback codecs. |
| [workbench_review/](workbench_review/AGENTS.md) | Implementation pieces for `workbench_review.rs`: Canonical structured-review codecs with bounded collection allocation. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
