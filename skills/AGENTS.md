# skills — navigation

Canonical skill packages, discovered recursively within each package and embedded by the
[product-runner build](../crates/app/peritus-product-runner/build.rs). Read [README.md](README.md)
for the package format and automatic registration contract.

| Folder | Role |
| --- | --- |
| [architect/](architect/AGENTS.md) | Repository-grounded implementation designs and delivery slices. |
| [computer-automation/](computer-automation/AGENTS.md) | Instructions for operating real browser and desktop interfaces. |
| [developer/](developer/AGENTS.md) | Implementation and repair guidance shared by writers and fixers. |
| [engineering-workflow/](engineering-workflow/AGENTS.md) | Detailed literal-contract, data, provenance, and acceptance guidance. |
| [real-testing/](real-testing/AGENTS.md) | Actual-interface behavioral acceptance, exploration, and failure replay. |
| [reviewer/](reviewer/AGENTS.md) | Independent review against explicit requirements and concrete evidence. |
| [rust-reliability/](rust-reliability/AGENTS.md) | Rust ownership, async, recovery, and reliability guidance and references. |
| [web-research/](web-research/AGENTS.md) | Web discovery, original source reading, citations, and retained-page reuse. |

The [runtime catalog](../crates/app/peritus-product-runner/src/bundled_skills.rs) owns lookup;
[tool readers](../crates/app/peritus-product-runner/src/developer_tools/bundled_skill.rs) own paged
model delivery. Execution and permissions remain with the host.

Navigation: [Repository map](../AGENTS.md).
