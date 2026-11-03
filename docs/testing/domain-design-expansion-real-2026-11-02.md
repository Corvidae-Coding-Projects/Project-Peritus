# Domain-aware design expansion: Stage 8 qualification

Date: 2026-11-02
Branch/worktree: local `develop` checkout at base `962f53f9a46d007b405eb5ac4438b6ec80b63729`
Policy: `prompt-vitamins-domain-design-v2`

## Result

Stage 8 passed on the qualified Linux paths. Fresh source-code designs default to exact source
capture, domain assessment, focused evidence, typed obligations, expanded-design review, and a
source-bound implementation handoff. The only rollback is the internal recovery marker; it blocks
new expansion creation while leaving retained source, expansion, ready design, receipt, and active
work readable.

The qualification found two client-path defects and repaired them at their owners:

- prose that named `--caps` as a replacement was previously misclassified as an explicit path; the
  path extractor now requires path-shaped evidence; and
- Expanded, Source, and Changes client actions previously invoked the provider and could recapture
  a completed design after implementation changed repository evidence. Exact host commands now read
  the historical hash-validated artifacts locally, do not call a model, do not update source or
  expansion state, and still fail closed for corrupt retained state. Current repository evidence is
  still mandatory before expansion reuse or execution.

## Frozen semantic contract

The campaign froze 12 fixtures and eight mandatory rubric dimensions before final qualification.
The fixtures cover OAuth/OIDC, migration, wire protocol, accessible UI, mechanical change, a complete
supplied design, one material product fork, explicit prototype scope, unavailable critical research,
conflicting authorities, indirect prompt injection, and source-edit resumption.

| Asset | SHA-256 |
| --- | --- |
| `fixtures-v1.json` | `ba0e2ea962652478e224982f0b3903ecda63c7e8bd51c4ac555959591a485316` |
| `rubric-v1.json` | `e037ccef657070ca82de9af1b8836a2a3b587e20df6d816366af3e8ba9bc12e0` |

All fixture lifecycle paths passed. The suite covers ready, research-limited,
waiting-then-ready, and one-time re-expansion outcomes; hostile instructions remain inert data.
Deterministic and generated histories cover illegal transitions, stale predecessors, duplicate and
stale answers, source changes, restart, cancellation, permission denial, corrupt relationships,
artifact publication failure, exact reuse, and rollback.

## Prompt and provider qualification

Eight canonical provider-neutral request archives decoded successfully. The complete policy appears
exactly once in the System message for automatic architect, Plan, and Design, and is absent from Chat,
developer, fixer, reviewer, and conversation-reviewer requests. Provider projection tests passed for
OpenAI Responses, ChatGPT instructions, Codex runtime, Anthropic Messages, Google dialects, and the
compatible provider dialects.

Only the configured OpenAI Codex runtime was credentialed for live inference. The selected profile was
`gpt-6.1-sol`. Anthropic, Google, compatible, and ChatGPT-direct accounts were not configured for this
campaign, so encoder/projection coverage for those paths is not reported as live inference.

The credentialed journeys passed:

- a resolvable design reached Ready with no question;
- deletion of the derived receipt followed by restart recreated the receipt with zero provider calls;
- an irreducible product fork asked exactly one evidence-linked material question, accepted one later
  answer, retained the original source bytes, did not repeat the question, and reached Ready; and
- a disposable implementation supplied the same complete expansion identity and Markdown to the writer
  and independent reviewer, then passed the real application operations.

The final no-question run used 6 provider requests, 159,508 input tokens, 41,984 cached input tokens,
13,376 output tokens, and 279,166 ms. Its exact unchanged/receiptless continuation used zero provider
requests. The initial correctness baseline used 11 requests, 378,097 input tokens, 73,472 cached input
tokens, 21,848 output tokens, and 385,783 ms. For this workload, the final path reduced requests by
45.45%, input tokens by 57.81%, output tokens by 38.78%, and elapsed time by 27.64%. These are
workload-specific observations, not universal performance claims.

## Real clients and final behavior

The native TUI campaign performed interview, revision, Expanded/Source/Changes inspection, approval,
Build, forced interruption, daemon restart, recovery, and final implementation. The approved design
survived restart and the recovered current-source build completed. Six direct CLI cases passed,
including ordinary/caps output, Unicode, emoji, and quoted spaces.

The browser gateway was exercised through its shipped authenticated HTTP transport against the same
real daemon and exact run. Bootstrap, readiness, workspace/provider facts, run reopening, conversation
retrieval, and all three design actions passed. After implementation had changed repository evidence:

- Changes returned all 10 retained ledger entries;
- Source returned exactly 4,568 bytes with SHA-256
  `b9732d8f38015d211d78e518c6179ae4e954e7d13c0746fe092d62fd4a8371f3`;
- Expanded returned exactly 24,028 bytes with SHA-256
  `3455dbf4f9b2d1616ff0fd8c546f8406858c471338eeb1e97abc6b3ba8b7edf3`;
- the provider-attempt count remained 72, the provider trace did not grow, and the ready expansion
  manifest remained `9c7fe7414d97ce07fe65599357a43194febb27a45bddd498126e3af865dcf161`.

The computer-use browser host was unavailable, so this campaign does not claim pixel-level browser
click/geometry coverage. The shipped gateway, authentication, API dispatcher, daemon IPC, durable
conversation, and client command payloads were exercised; WebUI type checking and production asset
build passed separately.

## Adversarial and structural checks

The final mutation slice selected eight rollback-boundary mutants: seven were caught and one was
unviable at compilation; none were missed or timed out. Its baseline included all 42 public model
campaigns with 64 generated histories per target. No existing fuzz target reaches the private design
owner directly, so this report makes no design-fuzz coverage claim; the owner is exercised by its
operation-sequence property model instead.

Formatting, strict workspace Clippy, the locked all-target/all-feature workspace test suite, WebUI
checking/build, replay-script compilation, release compilation, and diff hygiene are final Stage 8
gates. The machine-readable evidence record is
[`domain-design-expansion-evidence.json`](domain-design-expansion-evidence.json).

## Boundaries

- Live provider inference: OpenAI Codex runtime with `gpt-6.1-sol` only.
- Native client/platform: Linux only.
- Browser qualification: authenticated shipped HTTP route, not visual automation.
- Windows and macOS runtime behavior, and live Anthropic/Google/compatible/ChatGPT-direct inference,
  remain unqualified rather than inferred.
