# Candidate navigation

## Responsibility

The parent `../candidate.rs` combines managed runtime candidates with the product runner's in-place task scope. Managed preimages, export, and discard implementation now live in [the workspace runtime](../../../../runtime/peritus-workspace/src/task_candidate/AGENTS.md); the parent reexports that implementation for compatibility.
## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.
