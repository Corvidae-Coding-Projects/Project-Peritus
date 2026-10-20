# Bundled skills

This directory is the canonical source for every bundled Peritus skill. The product-runner build
script discovers packages here and embeds their original resource bytes. No Rust skill registry,
resource list, or tool-schema enum needs updating when a package or file is added.

## Add or update a skill

1. Create `skills/<skill-name>/SKILL.md`. The directory name is its stable runtime identifier.
2. Describe when to use it in optional YAML front matter, using `name` and `description`; write
   its instructions below. Without a description, the first Markdown heading is used.
3. Put references, scripts, agent metadata, licenses, images, or other resources below that same
   directory. Every regular file is included recursively, with exact bytes and portable `/` paths.
4. Rebuild Peritus. Cargo watches the whole skills tree, including additions and deletions.

For example:

```text
skills/my-skill/
  SKILL.md
  references/example.md
  scripts/helper.py
  assets/example.png
```

A minimal document looks like:

```markdown
---
name: my-skill
description: Explain which tasks benefit from this skill.
---

# My skill

Instructions for the model.
```

`SKILL.md` must be UTF-8. Front matter, when present, must be valid YAML. Other resources may be
binary. Symbolic links and special files are rejected so a package cannot silently embed files
outside its source tree. Root documentation and directories without `SKILL.md` are not packages.

## Runtime discovery and reading

Agents use `skill_list` to discover names and descriptions, then `skill_list` with `skill` to list
that package's resources. `skill_read` accepts any discovered skill and exact resource path.
Text pages preserve UTF-8 boundaries; binary pages use `encoding=hex` with original byte offsets.
Pages include the full resource size, SHA-256 and continuation offset. No installed source directory
is required: everything comes from the binary's immutable bundle.

The native `peritus_product_runner::bundled_skills` API exposes the same catalog, original resource
bytes, and paginated readers. Core role policy stays concise and refers to guidance on demand;
adding a skill does not inject every document into every prompt. Existing host activation such as
Rust reliability remains in place. Reading a resource does not execute scripts or grant authority.

## Packages

| Package | Purpose |
| --- | --- |
| [architect/](architect/AGENTS.md) | Repository-grounded implementation designs and delivery slices. |
| [computer-automation/](computer-automation/AGENTS.md) | Instructions for operating real browser and desktop interfaces. |
| [developer/](developer/AGENTS.md) | Implementation and repair guidance shared by writers and fixers. |
| [engineering-workflow/](engineering-workflow/AGENTS.md) | Detailed literal-contract, data, provenance, and acceptance guidance. |
| [real-testing/](real-testing/AGENTS.md) | Actual-interface behavioral acceptance, exploration, and failure replay. |
| [reviewer/](reviewer/AGENTS.md) | Independent review against explicit requirements and concrete evidence. |
| [rust-reliability/](rust-reliability/AGENTS.md) | Rust ownership, async, recovery, and reliability guidance and references. |
| [web-research/](web-research/AGENTS.md) | Web discovery, original source reading, citations, and retained-page reuse. |

Build owner: [build.rs](../crates/app/peritus-product-runner/build.rs). Runtime owner:
[bundled_skills.rs](../crates/app/peritus-product-runner/src/bundled_skills.rs).
