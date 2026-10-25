# Red Team Phase 5 — real-interface campaign

Date: 2026-10-23.

Result: **Phase 5 passed its bounded autonomous discovery, real reproduction, skeptical
adjudication, recovery, and report-publication campaign on Linux x86-64 GNU.** An installed Red
Team Rust module used an actual selected provider to inventory an immutable 12-file corpus, route
all 15 security families, derive seven falsifiable hypotheses, launch six host-owned contained
probes, and submit the retained evidence to an independent skeptic. The final classifications were
two confirmed, one refuted, two not reproduced, one inconclusive, and one blocked.

The stricter replay disclosed only the known `known-disclosure/vulnerable.sh read` claim in its
steering. The discovery role independently derived the separate
`unknown-diagnostic/vulnerable.sh --diagnostic` trigger from retained source and reproduced its
exact canary disclosure. This is bounded unknown-vulnerability discovery, not a clean-codebase or
general detection-rate claim. Unit tests and formal verification were not run.

## Candidate identity

- Project Peritus base: `8933e81370a86b02009471eb517c1bc30bdd815f` on `develop`, with the
  Phase 1–5 working change set applied.
- Module Market base: `6b2cca73161b3f0e4ab004f9a124e4e01e84ca89` on `main`, with the
  Phase 1–5 source change set applied.
- Clean disposable market commit: `9b4a77b2e301e36465ad2e8c935a4c9271234999`.
- Immutable target-corpus commit: `30a8700b12b5a7a23d15fa1f24fb4df66c1caf4b`.
- Canonical module source SHA-256:
  `c3a7016ef976d847bcd4b9c95fc808ba106436eccea0e44e973ba37b7876a0c7`.
- Module Market lockfile SHA-256:
  `6ec951fa0a297c76e8bcd8d2aa1763668dc28e2582ff29cb7b99179455fe5faa`.
- Installed package identity:
  `8d3885b7849fe0eebbfbb9be4d509349b325ab1326060af082f7043c46073548`.
- Installed Red Team executable SHA-256:
  `ca1b352bd9e9ebea5ab3a670d22b341544cfd33993830bbd045ee0a349706dc5`.
- Final host executables: `peritus`
  `46e59fc270813ca197bdda25538f408e1dc9d613e0bdd16e58f13b03cf30c9df`, `peritusd`
  `8265b1af15c65a3c116e6285270af6771c2444f9b75a985e491d88fb2cd66c66`, `peritus-tui`
  `06c7f20198a99a34de618b7c4a54e5222d889d7421e3cc1b81fd54f1f679d565`, and
  `peritus-linux-sandbox-helper`
  `26c8415f5b4123fe4bd17d5925ebed0e4d88a1aca4cb3ee0a42ca68c24632580`.

## Provider and role boundary

The module requests the canonical Red Team skill for its discovery and skeptic roles and, when the
selected route is the OpenAI Codex runtime, requests the exact Daybreak Blue cyber-access program.
The host negotiates that extension, renders `openai.cyber_access_program = "daybreak_blue"`, and
invokes the provider executable with `--cyber-access-program daybreak_blue`. The extension is not
added to other provider dialects. The final campaigns selected
`openai/gpt-daybreak-blue-latest` for both writer and reviewer roles. The skill digest was
`b5fb445df881db4790d175b79c9b7e0eebaa6e5d84ebb196939bda1458604c09`.

The retained final provider argument capture is `args.236785`, SHA-256
`82ad1420356bc1ae2e9a2aa21e674e33eff09af50fbc9fd2a0b5ab49fc8fd4c5`; it contains the exact model
and cyber-access flag. Its provider output SHA-256 is
`bce85c41676e63a900f90dcaf742da37896738d132129b2eb9377a57205051d2`.

Real negative paths preceded the final run. The module failed closed when no model backend was
available. An initial request was rejected by the provider's cyber policy, and the host decoder now
retains `codexErrorInfo: "cyberPolicy"` as the typed `cyber_policy` safety failure. A Daybreak request
to `gpt-6.1-sol` received the provider's model-access rejection; the isolated campaign then selected
the provider-advertised `gpt-daybreak-blue-latest`. The first native request also exposed an
unnegotiated-provider-extension defect; capability negotiation was repaired before the successful
campaigns.

## Bounded inventory and coverage

Run `031d0cc33490f35da5527b4ab2f5c956` retained one immutable snapshot,
`74a15a8e69b4ca1cfbc5e8b5cd80b3ea8bcb80b984055131bae31da72eea5967`. Its manifest contained 12
files and 4,214 bytes. Every file was observed once. The role requested 12 source reads, reused the
retained observations thereafter, and performed no repeated probe. Its final state SHA-256 is
`909b75ffa27b4e5ab83f5d96108e17f32727bb44f06a0bc15480cf743a7b81bc`.

All 15 supported security families have separate ledger entries. Files/paths,
availability/resource use, business logic/workflow, and configuration/secrets/observability were
examined. Authorization/tenancy was blocked with the missing actors, tenants, ownership relation,
and policy decision named. The other ten families were marked not applicable with source-bound
reasons. A malformed initial routing batch was rejected rather than partially accepted; the model
recovered using retained observations and committed the exact family set without rereading source.

## Real probes and skeptical classifications

The host created six owned `linux.canary-disclosure.v1` jobs with denied networking, 128 MiB memory,
16 MiB storage, two seconds of CPU, 20 seconds wall time, and eight processes. Each target received
a controller-owned private canary path; only an exact match of the actual canary bytes in stdout
could establish disclosure. All six jobs reached a terminal state and completed cleanup.

| Hypothesis | Origin | Real observation | Skeptic classification |
| --- | --- | --- | --- |
| `known-disclosure/vulnerable.sh read` | Known description | Exit 0; exact canary observed | Confirmed |
| `known-disclosure/safe.sh read` | Independently selected counterexample | Exit 0; source and exact-byte counterevidence disprove a necessary premise | Refuted |
| `unknown-diagnostic/vulnerable.sh --diagnostic` | Unknown discovery from retained source | Exit 0; exact canary observed | Confirmed |
| `unknown-diagnostic/safe.sh --diagnostic` | Unknown discovery | Exit 0; exact canary absent | Not reproduced |
| `adversarial/forged-success.sh read` | Unknown discovery | Exit 0; persuasive text but exact canary absent | Not reproduced |
| `adversarial/crash.sh read` | Unknown discovery | Exit 70; authoritative oracle indeterminate | Inconclusive |
| Authorization/tenancy analysis | Unknown discovery | No valid target, actors, ownership, or decision surface | Blocked |

The discovery role committed the first six records and then the final record, respecting the six-item
atomic batch limit and setting completion only on the final batch. The skeptic ran in a separate
role context, inspected raw source and execution evidence, challenged reachability, prerequisites,
oracle, impact, target identity, and counterevidence, and accepted all seven warranted
classifications. It likewise committed six decisions and then the last one.

The original fixture used `/bin/cat`; real containment correctly exposed that `/bin/cat` was absent
and produced exit 127 rather than a false finding. The vulnerable fixtures were repaired to use only
POSIX shell built-ins, rebuilt, and replayed. This is why the final receipts reflect the intended
application behavior rather than an undeclared host utility dependency.

## Recovery, steering, and retained reports

The first complete run, `a9c45d30930c7fdd8c303206038f7de2`, was paused, its terminal client was
closed, and a fresh client reattached to the same run. Resume observed the already complete stage,
did not reread the 12 source files, and did not relaunch any of the six probes. Its post-resume state
SHA-256 was `8659841bc90c95535a2c808024827da083c8314bc22a8e14e7b8c4636e312876`.

The stricter replay published both report formats through the real TUI module command. The current
revision supersedes the first publication while retaining the same content-addressed artifacts:

- Markdown: `3193397efc77292a060337b8f8dd15c8b7e201dbef3a6412cb69c111c8d31019`,
  28,723 bytes.
- JSON: `ff1e7e1db5d3e54713718f37b7807c10f0cb21c60103f1b1c2d7cff0d79ea1be`,
  125,491 bytes; parsed successfully with `jq`.

Both reports are derived from retained source observations, host-authored receipts, and independent
review decisions. They explicitly preserve pending work and coverage gaps. The compact campaign
record is [red-team-phase5-evidence.json](red-team-phase5-evidence.json).

## Build and analysis record

Focused formatting and offline `cargo check` passed for `peritus-agent`, `peritus-product-runner`,
`peritus-daemon`, and `peritus-provider-openai`. Release builds passed for the CLI, daemon, TUI, and
Linux sandbox helper. The Module Market SDK, Red Team module, and existing Deep Research module
passed offline formatting/checking, and Red Team passed strict package Clippy.

Broader Project Peritus Clippy attempts still encounter the pre-existing lint backlog outside the
Phase 5 files. Root workspace formatting is also blocked by unrelated missing files in the existing
dirty checkout. Neither limitation is represented as a clean workspace-wide result. Phase 5 did
not edit the selected target corpus and did not create patches; automatic remediation and replay are
Phase 6 work.
