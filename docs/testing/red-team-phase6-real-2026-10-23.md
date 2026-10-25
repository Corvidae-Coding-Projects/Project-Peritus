# Red Team Phase 6 — real-interface campaign

Date: 2026-10-23.

Result: **Phase 6 passed its bounded copy-only repair and delivery campaign on Linux x86-64
GNU.** The installed native module investigated the disposable Phase 5 corpus, retained two
confirmed disclosures, proposed exact-preimage replacements, and handed every effect to the
host-owned replay adapter. Both final revisions passed attack, legitimate, malformed, and adjacent
cases and an independent reviewer. The host then reread the original 12-file fixture and observed
the starting bytes and executable bits unchanged.

This evidence is bounded to the selected corpus, declared cases, Linux host adapter, configured
Daybreak provider, and recorded receipts. It is not proof that an arbitrary project is secure or
that the proposed patches cover unrecorded behavior. Unit tests and formal verification were not
run.

## Candidate identity

- Project Peritus base: `bfe3189e20ace3bdfa36b0922f999a6c75ba56ee` on `develop`, with the
  Phase 6 working change set applied.
- Module Market base: `dedb2155a0d937e559746591ae4bd66d55ca6e6f` on `main`, with the
  Phase 6 working change set applied.
- Clean disposable market commit: `2d41e3013649e31842903d007721f2ced63777f4`.
- Behavior-tested source-tree SHA-256 before this implementation record was added:
  `cdc8da373ebd443b68c6758eeaa7c04cf9de2fed0dba28475971e4b8f2cc0804`.
- Final source-market tree SHA-256: `0096577874648126f287fb1f5ec4566aabac95f7cd81b61ccb194aef3e956adb`.
  The only bytes added between those identities are the Phase 6 implementation record in
  `.design/red-team-phases.md`; module code, SDK, resources, lockfile, and package inputs are
  identical.
- Source-built executable SHA-256:
  `63b94b7874ba43910e1c02a886b270fa10fe5f9d2ae6efd48b22c22494eca966`.
- Source-built installed package SHA-256:
  `f95e1789c9072c45f0496bafc03aeeaced3de651722275041c6098e253b67d90`.
- Development executables: `peritus`
  `46e59fc270813ca197bdda25538f408e1dc9d613e0bdd16e58f13b03cf30c9df`, `peritusd`
  `980f617a7e9bb8048d6080d9db144963fc1cb6ad1328bd0e5774003f8faa843d`, and `peritus-tui`
  `06c7f20198a99a34de618b7c4a54e5222d889d7421e3cc1b81fd54f1f679d565`.

## Observed repair campaign

The final rebuilt-module run retained 14 host job receipts: six investigation probes and eight
copy-patched replay cases. The known `read` disclosure and independently discovered diagnostic
disclosure each received a separate exact diff. For each patch, the host reconstructed a fresh
copy from the immutable source authority, required the original digest, generated the unified
diff, executed the changed copy inside the private filesystem/deny-all-network sandbox, and
required both canary nondisclosure and the declared positive output.

The final plans passed these independently declared categories:

- known disclosure: lowercase `read` denied, `status` remained ready, empty input remained
  unsupported, and adjacent uppercase `READ` remained unsupported;
- diagnostic disclosure: `--diagnostic` returned the fixed public health response, `status`
  remained ready, empty input remained unsupported, and adjacent `--Diagnostic` remained
  unsupported.

The two validated patch artifacts are
`da40c56cedd2e7f36a90af29c0f9734012a396d286fbaea86186e25748ae691c` and
`daa86d6f367fc45be04f1af56923f74d79b3745c021fc855a92b64975efb5ea4`.
The final original verification recorded no added, changed, or removed paths and an observed
manifest digest of `46c298d635a8e8687e29b65467d668422fbbf98771192846428b439c604dae72`.

## Recovery and live control

Real testing exposed and repaired four integration defects before the final pass:

1. a fixer could durably advance to replay before `model_result`, causing stage-based request-key
   validation to reject its own completed role;
2. a same-module pause could arrive between a revision precheck and plugin admission, producing
   `module-authority-changed` instead of a durable control handoff;
3. resume with a pending host request reissued the request before clearing the durable paused flag.
4. an active same-module control at the report-checkpoint boundary could still take the ordinary
   supersession path rather than being admitted as a live control.

The final behavior binds model tools to their original revision, admits only same-module `pause`,
`resume`, or nonempty `steer` controls during active work, and explicitly rebinds the next plugin
call after durable command admission. A real TUI detached and reattached during repair, paused with
a replay request pending, resumed without losing or duplicating it, and continued in a fresh
daemon/plugin process. The rebuilt-module continuation proved the first resume cleared the paused
flag and completed the remaining repair.

A separate real WebUI run exercised the rebuilt host with two exact-patch edge files: one source
without a final newline and one CRLF source. Both copy-patched jobs completed through the Linux
sandbox. Their host-authored diffs passed independent `git apply --check`, applied to fresh copies,
and reproduced the replacement bytes exactly, including the absent final newline and CRLF. The
selected originals remained byte-identical. The compact receipt is
`/tmp/peritus-phase6-edge-20261023/evidence.json`, SHA-256
`4a4f01553afa5443360ca990218bcb4aa55834c2fce76d30f6f64a1beae308f6`.

## Delivery and source-market path

The completed fresh-process run exported Markdown report
`1257e0e60f14dd682089dc245c1417240d640383f27e0f4c5cd427e295ba0a6f`, JSON report
`146615933e5b79938a2d254efad9aa14fdbee6a7bc6186ca5b176bcdae0280ed`, the two exact patch
artifacts, and replay bundle
`e8bc62adf778066094dac13d82f9b3262e7f7f4eabcb7644820740a641190dfe`.

A separate clean Git checkout was discovered through the real module-store TUI, built by the
host-owned offline source-build path, installed outside Git, explicitly enabled, selected, and
launched. The built package reported `Red Team — Phase 6 investigation and repair`. The source
checkout remained clean and contains no compiled plugin artifact.

Compact evidence is in
[red-team-phase6-evidence.json](red-team-phase6-evidence.json). Raw disposable state and screens
remain outside the repositories under `/tmp/peritus-phase6-real-20261023q/` and
`/tmp/peritus-phase6-source-build/`.

## Build record and bounded gaps

Focused `cargo check` passed for the changed Project Peritus SDK, runner, and daemon packages.
The Module Market SDK and Red Team package passed formatting, `cargo check`, and strict package
Clippy, and both development and source-market optimized builds completed. Strict Project Peritus
package Clippy still encounters the repository's pre-existing unrelated lint backlog in web,
guided-design, and read-reuse files; it is not represented as a clean whole-package result.

No macOS or Windows replay adapter was exercised. The retained report correctly says its patch
validation is case-bounded, and families without a suitable host adapter remain analysis-supported,
inconclusive, blocked, or unexamined rather than being claimed safe.
