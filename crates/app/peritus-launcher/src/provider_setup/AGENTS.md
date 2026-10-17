# crates/app/peritus-launcher/src/provider_setup — navigation

## Responsibility

Implementation pieces for `provider_setup.rs`: First-run provider selection and focused repeat-launch repair.

## Files in this folder

| File | Role |
| --- | --- |
| [connection.rs](connection.rs) | Provider settings actions and explicit small inference checks. |
| [direct.rs](direct.rs) | Hidden credential entry and direct-provider settings prompts. |
| [install.rs](install.rs) | Human-approved installation at the selected account-provider boundary. |
| [models.rs](models.rs) | Provider-owned model menus shared by account and direct setup. |
| [reviewer.rs](reviewer.rs) | Select and retain the optional reviewer provider/model override during setup and repair. |
| [selection.rs](selection.rs) | Provider-set and default selection parsing with recoverable prompts. |

Owning module: [provider_setup.rs](../provider_setup.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
