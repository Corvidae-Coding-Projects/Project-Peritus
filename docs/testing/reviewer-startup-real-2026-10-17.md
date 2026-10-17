# Reviewer model startup selection — real testing

## Confirmed defect and correction

Published v0.1.0 completes provider setup after selecting the general account
model. It never offers a reviewer provider/model. The TUI initializes writer,
reviewer, and fixer from the same default, even though the existing daemon API
supports independent reviewer model choices.

First-run setup and `peritus providers` now save an optional reviewer preference.
`0` follows the ordinary default. An explicit choice retains its enabled provider,
exact model ID, and whether the user entered it manually. The launcher supplies
that choice to new TUI conversations through the existing execution settings;
writer and fixer stay on their existing choices. Old state without the preference
continues to follow the default.

## Observed scenarios

| Actual operation | Observed result |
| --- | --- |
| Published v0.1.0 provider setup | Missing menu reproduced: setup exits after the general model choice. |
| First-run setup with different coding/review models | Saved coding `gpt-6-luna`, reviewer `gpt-6.1-sol`; fresh TUI Review mode shows the reviewer choice before opening a model picker. |
| Provider settings reopened; press Enter | Reviewer model/provenance and ordinary model retained. Official executable pins may reconcile with the current environment independently. |
| Invalid provider numbers/text and malformed manual model | Reprompted; valid subsequent selection saved. |
| Cancel during reviewer model selection | Setup exits unsuccessfully; durable product state unchanged. |
| Select `0` | Reviewer override removed; ordinary coding model unchanged. |
| Select Claude for review while Codex remains default | Saved reviewer provider/model independently; no coding-provider/model replacement. |
| Remove the selected review provider | New default/follow-default choice saves without a disabled reviewer binding. |
| Official Codex reports signed out; decline repair | Saved reviewer override clears with an explanation; offline TUI opens after the owned prior daemon is gracefully stopped. |
| Fresh offline setup | No reviewer prompt or dangling reviewer choice; TUI opens. |
| Actual read-only review of a disposable Python file | Codex `gpt-6.1-sol` reads the file and returns its greeting behavior. Daemon run metadata retains reviewer model only, with writer/fixer configured defaults, and records the requested review model. Fixture contents unchanged. |
| Reset startup reviewer to `0`, then reopen the existing run | Saved reviewer model and prior review response return unchanged, without another inference request. |

The Rust executable was built with
`cargo build --locked --release --jobs 2 --package peritus-cli --bin peritus`.
The final CLI SHA-256 was
`7996e181ac1a836c0c656b09e1ed28de2d7b1cc8e38a8738b817d1eb17a652e8`;
the retained menu replay and actual Codex review passed again on that artifact.
The isolated native package paired the changed CLI/embedded TUI with the released
v0.1.0 daemon/helper. No daemon API or provider-adapter format changed. Separate
XDG roots, workspace, terminal processes, and daemon instances were used; existing
user installations and running application jobs were not changed.
All owned disposable daemons were shut down through the normal CLI afterward.

No unit-test, formal-verification, or repository verification suite was run.
Coverage is the native Linux startup/settings/TUI path with real account-model
discovery and a real Codex review. Cross-provider selection was exercised; this
campaign does not claim live inference coverage for every API/hosted provider or
native execution on Windows/macOS.

## Retained regression replay

An already signed-in official Codex runtime must be discoverable. The replay uses
real metadata discovery and manual IDs; it does not request inference or alter
credentials. The output directory must be new.

```sh
uv run --with pexpect python docs/testing/reproducers/reviewer-startup.py \
  --binary target/release/peritus --output /tmp/peritus-reviewer-menu-replay
```

The script drives the actual terminal entry point, then reads the saved product
state. The original executable fails to reach the reviewer prompt. Captured
terminal transcripts remain in the supplied disposable output directory.
