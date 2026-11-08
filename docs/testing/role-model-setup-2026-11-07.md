# Explicit role model setup

Startup previously offered only an optional reviewer model and omitted the fixer's model and
both roles' effort. A healthy installation skipped setup thereafter. Direct `/model` IDs also
required a previously loaded catalog, so valid commands could be rejected. Missing selections
then interrupted work when execution finally reached review or repair.

Startup now collects complete writer/reviewer/fixer provider, exact model, and effort defaults.
An installation without complete defaults is prompted once. Repeated launches retain completed
choices; provider settings permit explicit revision. Copying the writer is an explicit displayed
choice. Existing conversations retain their durable selections. The old reviewer preference is
retained as an input to migration, with effort still requiring selection. Provider credentials
and execution authority remain in their existing owners.

`/model` (also `/models`) now opens a guided menu: choose Writer, Reviewer, or Fixer,
choose a provider-advertised model, then choose reasoning effort. Arrows and Enter operate all
three pages; no model ID needs to be typed or remembered. Escape goes back, and Escape on the
agent page cancels. Choosing the model stages it locally; the final effort confirmation saves
both settings in one update. `/effort` remains independently available.

The staged selection is bound to the current run, conversation, provider set, role, connection,
and unchanged composer. A changed destination cancels the menu's continuation. A removed model
returns to selection before any save. Requests in flight retain their original selection;
future turns use the acknowledged update. Direct commands remain optional shortcuts and do not
replace the menus. Missing catalog entries require an explicitly labeled manual command; no
request substitutes a different model or effort.

Delayed lookups bind to role, provider, run, conversation, connection, and unchanged composer.
Escape cancels their continuation; failed discovery restores the original command, and a new
draft is never overwritten. Existing conversations continue to be daemon-owned; the launcher
stores defaults only for future conversations.

## Verification

The retained replays in `discovery/proptest-praxis/owner/role_setup.rs` script the actual terminal
boundary, publish to the actual immutable-generation store, and reopen it. They cover complete
startup, distinct reviewer/fixer selections, legacy migration, explicit effort, cancellation,
invalid durable fields, and disabled providers.

`tui_role_models.rs` exercises actual slash parsing and correlated protocol responses for all
roles, one atomic update, preserved sibling roles, no optimistic acknowledgement, duplicate and
late replies, invalid IDs/arguments, and failed lookups. Ratatui's actual renderer checks that
all three agent settings, the model list, and the staged-model effort page appear at 80x24 and 120x30. Keyboard replays cover each agent, no partial save, Back/Cancel, failed discovery and retry, removed-model rejection, changed destinations, and standalone effort changes.

These replays verify local setup, persistence, client projection, and request construction. They
do not assert that every advertised effort works on every provider model; the provider remains
responsible for reporting support. The active Blue/XHigh run is independent live evidence of
the existing authenticated role update and subsequent execution path.

## Observed results

- All 20 launcher/TUI library checks passed, including the existing mode-switch regressions.
  `cargo clippy --locked -p peritus-launcher -p peritus-tui -p peritus-product-state --all-targets -- -D warnings`
  passed. The optimized CLI and standalone TUI built successfully.
- The actual `peritus providers` executable ran in a PTY with isolated XDG roots and real
  provider observations. Canceling before fixer selection left no partial defaults. Completing
  setup persisted three exact Blue/XHigh selections. A fresh process retained all three.
- A native TUI session connected to the existing daemon using a disposable, unstarted
  conversation. Entering `/model` opened agent, model, and effort menus in order. Only arrows,
  Enter, and Escape were used; no model IDs were typed. Back from effort and model selection
  preserved the prior settings. The final save changed only fixer to the chosen Sol/High pair,
  while writer and reviewer retained Blue/XHigh. The standalone `/effort` menu still opened.
  The client exited cleanly, and independent durable readback confirmed the active run's
  selections were unchanged.

Private native transcripts, terminal screens, CLI state generations, and replay drivers are
under `/home/doll/.cache/peritus-watch-20261107/`; the guided-menu screen captures and terminal
transcript are under `/home/doll/.cache/peritus-model-menu-native-20261107/`. The temporary native TUI driver source was
archived in the watch directory and removed from the release source tree. The retained owner replays above are
the repository regression entry points.

The verified CLI and TUI were installed atomically for the local account. Backups and executable
digests are in `/home/doll/.cache/peritus-model-menu-install-20261107/installed.json`. The daemon's
PID and executable digest were unchanged, and the installed CLI successfully queried its status.
An already-open client must be reopened to load the new interface; the daemon continues its run.

Verdict: ready for local use within the tested terminal workflows. Provider sign-in was retained;
this campaign did not establish model-specific effort support across every provider.
