# Red Team Phase 8 — adversarial hardening and source publication

Date: 2026-10-24.

Result: **Phase 8 passed its bounded Linux acceptance campaign, and Red Team 0.2.0 is ready for
source publication.** The final module includes a saved MIT-licensed hostile corpus, fails closed
against deceptive targets and invalid authority, survives the exercised lifecycle and resource
failures, produces independently reviewed copy-only patches, and leaves the selected original
repository unchanged.

Unit tests and formal verification were not run. This campaign operated the real launcher, daemon,
TUI, native module process, Linux helper, source-build scope and actual provider. Prior Phase 7
WebUI evidence is reused only for the unchanged generic projection boundary.

## Candidate identities

- Project Peritus base: `bfe3189e20ace3bdfa36b0922f999a6c75ba56ee` on `develop`, with the
  Phase 6–8 host changes applied.
- Module Market commit: `84948422ef3135ed720fc44f03ce3008a588578c` on `main`.
- Module version: `0.2.0`.
- Canonical source digest: `26e58208f4ce9c75d83cf727c5ee9d9310bf991bdb7ca910df480c08395cc9d6`.
- Lockfile digest: `8ad72c65bb0bc20f31fd1f12637f28b107cdb970fd32d5b56600a8e00207beff`.
- Maintainer-built native executable: `9d50e246db7ed8ddd45602490941c5a3d5180a7cfda66ab85fa13866e839e21c`.
- Clean host-built executable: `c9731d9cf8f9db0c367ce6f5609ebd0033645be41c26c7356cdc493265a9e45f`.
- Clean installed package: `f681de96fd44c6fae96c780960fa4183c9724d76d8b36e2fec17bf3bfce1b6a0`.

The different maintainer and source-build executable digests reflect their different absolute Rust
source paths; the catalog binds and reports the clean host-built executable and package identities.

## Defect found and repaired

The first copied-browser run reached the actual provider and host adapter but produced no confirmed
findings or patches. Discovery had encoded an argument vector—sometimes including
`/tmp/peritus-owned/private-value`—inside `attacker_input`, although the host contract expects one
literal second argument and supplies the private canary path separately. The host correctly ran the
literal malformed value and returned property-held or indeterminate receipts; the skeptic correctly
refused to fabricate a confirmation.

The fix makes the single-opaque-argument boundary explicit in the discovery role, function schema
and Rust admission logic. Encoded `argv` and the host canary path are rejected. The same copied
brownfield campaign then registered literal `read` and `--diagnostic` inputs, confirmed both genuine
disclosures and completed the full repair chain.

The failed pre-fix evidence remains under `/tmp/peritus-red-team-browser.cmmNjR/run/`; its evidence
and terminal SHA-256 values are `35f31482…` and `0434e395…`. The passing replay is under
`/tmp/peritus-red-team-browser-phase8.o1cxQg/run/`.

## Hostile containment and recovery replay

The real TUI replay passed 15 scenarios:

- out-of-scope symlink and parent traversal rejection;
- capability and exact source-scope binding;
- vulnerable target detection while safe, forged-success and prompt-injection targets could not
  manufacture proof;
- host file, network, home-directory and credential-environment isolation;
- explicit cancellation and descendant cleanup;
- timeout plus memory, storage and process pressure with retained terminal receipts;
- digest-pinned public HTTPS preparation and private/credential-bearing preparation rejection;
- live scope revocation cancelling owned work; and
- host-exit recovery retaining the interruption without relaunching the effect.

Its compact evidence SHA-256 is
`7f5bbe40df5c6d62bafe8649292d4872ab91ee99bff883f25612c1431bd3daf9`; the full terminal capture is
`d9c4e8ad36b7c0394aa55d4b4148d1e6d34f323e61d3d909d20c388e58ac3af7`.

A separate native-protocol replay created 0.2.0 state, reopened it in a fresh module process,
replaced it with malformed JSON and observed a structured rejection, then restored the exact valid
bytes and reopened successfully. Evidence SHA-256:
`0bafa1c49d1343629685b8ed1aa4a42642b0e166a5cebf1d37bc97e3569d0759`.

## Actual-provider brownfield campaign

The selected source was a disposable copy of `/home/doll/browser`, excluding its Git metadata,
build output and existing reproduction output. The original checkout's three pre-existing modified
paths remained exactly as they were before and after the campaign.

The real `gpt-daybreak-blue-latest` run retained one 127-file manifest, 31 source observations, 14
hypotheses, 22 host-owned jobs and receipts, 14 skeptical finding decisions and 16 reviews. It
deliberately left cryptography and key lifecycle unexamined. Results were two confirmed, seven not
reproduced, four refuted and one inconclusive—not a synthetic all-clear.

Both confirmed disclosures advanced automatically into copy-only repair. Ten patch revisions were
retained: eight intermediate applied-to-copy records and two validated-for-recorded-cases patches.
The final patches passed attack, legitimate, malformed and adjacent input cases plus separate
review. The run also passed client reattachment during repair and pause/resume while a replay
request was pending.

The final report identities are:

- Markdown: `60816bf6a3d303d95330cabd3c9212481a9d1c0900c82cb9886c1a78fba72bd7`;
- JSON: `a900c8c17a299c05612203e87ec1bcc51eb1aa215949f65bf51fe39ce53590ce`;
- exact patches: `433d59a2…` and `7176f76d…`; and
- reproduction bundle: `941838d3b135980ff671f4b17fccd6c86e73542a66c5b4cf83304c3e0b160ff1`.

The independent before/after path, byte and executable-mode manifests both hash to
`967029bd95d00285b721e17ee4d95ed8cc8cd8c49b54b3bf7153d24ffcdc175f`; no selected original path
was added, removed or changed.

## Clean source-market journey

A fresh clone of exact commit `8494842…` exposed source version 0.2.0 through the real TUI. The
host prepared locked dependencies, compiled offline with its network namespace denied, packaged
the exact declared resources and installed outside Git while disabled. The human-interface action
then enabled the package, prepared its use command, launched the installed native executable and
displayed the version-neutral Red Team investigation status.

The source-build record is job `229caf6e…`, recipe `d9634bfa…`, executable `c9731d9c…`, and package
`f681de96…`. No generated binary or build output is tracked below `modules/red-team`.

Formatting, locked compilation, strict package Clippy and the optimized native build pass for the
Module Market. Focused Project compilation and WebUI type/production builds are recorded after the
final host diff. The complete A1–A14 mapping and explicit limits are in
[red-team-phase8-acceptance-ledger.md](red-team-phase8-acceptance-ledger.md); compact identities are
in [red-team-phase8-evidence.json](red-team-phase8-evidence.json).
