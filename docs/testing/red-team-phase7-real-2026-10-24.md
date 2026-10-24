# Red Team Phase 7 — complete human-interface journey

Date: 2026-10-24.

Result: **Phase 7 passed its bounded Linux TUI and WebUI integration campaign.** The native Red
Team module now publishes one generic, exact-run projection containing concrete progress, every
available action, a bounded Markdown summary, and the selected export's name, byte count, SHA-256,
and stable local path. Both human interfaces expose that projection without a Red Team-specific
contract.

The exercised security campaign was the completed actual-provider Phase 6 campaign. Phase 7 did
not rerun inference merely to repaint retained facts: it replayed the same durable campaign through
the final host, TUI, and WebUI code, verified restart recovery, and independently read the exported
artifacts. A separate clean source checkout exercised discovery, isolated build, disabled install,
and explicit enablement. Unit tests and formal verification were not run.

## Candidate identities

- Project Peritus base: `bfe3189e20ace3bdfa36b0922f999a6c75ba56ee` on `develop`, with the
  Phase 6 and Phase 7 working changes applied.
- Module Market base: `dedb2155a0d937e559746591ae4bd66d55ca6e6f` on `main`, with the
  Phase 6 and Phase 7 working changes applied.
- Final clean source fixture commit: `5b4cfb53942cebd5b528653008c52b50afc74900`.
- Final market source digest: `7527a08982f92b0d6083dfbe332586b808cc6bdacd705c4e7190beeb7fc5e92e`.
- Source-built executable: `227cce268d7db5f2d1a6bd20860987160853280bc2be80ae08431bb5c55f82e1`.
- Source-built installed package: `367476aa3d8a083759db37c5041b48690892582c729bbc9485884bfa46c30cf5`.
- Optimized host binaries: `peritus` `66f5ceb191a79b80528affa5abd7d2ab9adc8f2bd8597aee9f3c293e1e37241b`,
  `peritusd` `cd84cf91129e9d8645b8606eb6128499de9306d0d9ed66da73c3bd0566aa21bc`,
  `peritus-tui` `3fb66c0826fa5d09c40f1205b798ee8673ed38688dca268a8f4433ea680455c5`,
  `peritus-web` `ea46054ca77b1e0fc70f70302e72fdcfb40d54772d950de5bfbdc550f2452fe5`,
  and Linux helper `c64d43021c3625fa7611d7ac9288c3f63bff9c443583cff11c3486fb05ee2bda`.

## Actual TUI journey

The real launcher, daemon, TUI, native module process, and Linux helper opened run
`0bd1b663311a319a48749b7c463975f1`. The module panel rendered all 17 actions—rather than silently
stopping at numeric shortcut 9—plus current progress, the durable summary, and the selected export.
Left/Right and Tab/BackTab select any action; Enter submits the selected action and digits remain
shortcuts.

The TUI invoked status, findings, exact patches/diffs, and report delivery. It then disconnected
gracefully, stopped and restarted the daemon, and reopened the exact run. The same 17 actions and
the same 34,488-byte Markdown report reappeared without duplicate publication. The visible export
path ended in `1257e0e60f14dd682089dc245c1417240d640383f27e0f4c5cd427e295ba0a6f.md`;
an independent file read produced that same SHA-256.

## Actual WebUI journey

The real browser client separately opened run `6a80cfd18b729c08ce3348106d36f6c9`, rendered the
completed campaign, and polled its exact-run projection. The module panel showed all 17 actions,
11 current source observations, six hypotheses, six probes, six adjudicated findings, 14 trusted
job receipts, ten patch revisions, eight patch replay receipts, zero unexamined families, and the
unchanged-original result.

The human selected **Patches and exact diffs**. The transcript displayed both complete unified
diffs, their root-cause explanations, review identities, and attack/legitimate/malformed/adjacent
replay outcomes. The human then selected **Export partial JSON report**. The panel displayed and
copied a stable 339,829-byte path ending in
`146615933e5b79938a2d254efad9aa14fdbee6a7bc6186ca5b176bcdae0280ed.json`; an independent
SHA-256 and JSON-shape check matched the visible identity. Restarting both daemon and Web gateway
preserved the same conversation, projection, export, and action set.

## Source market and failure paths

The WebUI refreshed from a clean Git checkout and visibly changed the Red Team source identity to
commit `5b4cfb53942cebd5b528653008c52b50afc74900` and digest `7527a089…`. **Rebuild and
install** advanced through dependency preparation and offline compilation inside the bounded Linux
source-build scope. It installed package `367476aa…` disabled, as required; only a separate
**Enable** action activated it. The build record binds the catalog commit, source digest, recipe,
target, executable, and package identities.

The final native executable was also invoked with no host profile. **Check compatibility** returned
the actionable message that the host does not advertise the versioned native-module contract and
must be updated before source selection. **Start** without a source named the exact `scope`
prerequisite. A retained real WebUI host-job replay separately records fail-closed isolation as
`PERITUS-SANDBOX-BACKEND-001: backend lacks required features`; the host did not run the payload.
An invalid patch dependency was visibly rejected before it could start a host effect; no original
mutation occurred. The completed campaign retained eight superseded copy-only patch revisions
before two exact patches passed all recorded replay categories. Report-now behavior was exercised
by both Markdown and JSON exports, and the Phase 6 real-provider pause/reconnect/resume campaign
remained the recovery oracle for provider interruption.

## Original preservation and checks

After the TUI, WebUI, restart, export, and source-build journeys, all 12 selected application paths
still matched the original SHA-256 and executable-mode manifest. No patch was applied to the
selected original.

Focused formatting and `cargo check` passed for the changed Project Peritus SDK, module, runner,
daemon, TUI, and Web packages. The WebUI passed `svelte-check` with zero findings and its optimized
production build. The Module Market passed workspace formatting and strict package Clippy for
`peritus-red-team`. Whole-workspace Project formatting remains unavailable because the root Cargo
graph still names two deliberately removed test fixtures; focused formatting is the applicable
source check. Earlier project-wide strict Clippy stopped in the documented pre-existing
`peritus-modules` lint backlog, not in the Phase 7 progress API.

Compact identities and scenario results are in
[red-team-phase7-evidence.json](red-team-phase7-evidence.json). Raw disposable state remains outside
the repositories under `/tmp/peritus-phase7-tui-final/`,
`/tmp/peritus-phase6-real-20261023q/`, `/tmp/peritus-phase6-edge-20261023/`, and the clean checkout
`/tmp/peritus-phase7-market.GDrbXI/`.

This is a bounded Linux integration result, not a cross-platform execution claim or proof that an
arbitrary application is secure. Phase 8 adversarial hardening has not begun.
