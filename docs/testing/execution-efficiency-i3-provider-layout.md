# I3 completion: once-only provider schemas

Status: complete for I3, phase four. I0–I3 are complete; I4–I6 remain incomplete.
OpenCode now defaults to the once-only layout. Explicit `opencode_request_layout = "legacy"`
on a hosted route, or the matching library builder option, restores the previous projection
for future requests against current history.

## Implementation and boundaries

The full declared argument schemas and descriptions remain in the strict system result schema.
The prompt catalog now contains names and selection/count metadata, removing its duplicate
definitions. The system bytes, schema variants, required fields, additional-property controls,
minimum/maximum calls, model/options, role labels, transcript order and host admission are
unchanged between selections. Specific selection keeps its existing selected-tool-only schema.
Automatic, required, none and parallel controls retain their existing behavior. No tool is
removed, and there is no new checkpoint, transcript or application-protocol version.

The library default, daemon route declaration and registry builder carry the same selection.
Unknown values/fields and settings on another adapter reject. A real replay found that
`select_model` reconstructed the provider without carrying its layout: explicit legacy silently
became once-only after selecting even the same model. The selected provider now retains that
setting. The prior binary and actual failure remain in `i3-reselect-before`; the permanent
`reselect` campaign checks both selections.

Native tools, plugins and MCP remain disabled. The existing completed-inert-turn correction
gets at most one additional inference, with no rejected host dispatch. Native failure,
incomplete/ambiguous acceptance and cancellation keep their existing certainty/retry rules.
There is no automatic layout fallback or blind resubmission of an admitted request. Library
hosts use `OpenCodeRuntimeProvider::with_request_layout(OpenCodeRequestLayout::Legacy)` for
explicit rollback; same-model selection preserves it. No history downgrade is required.

## Correct accounting

Qualification uncovered a second defect: native `tokens.input` is fresh input, while normalized
C5 input must include cache-read and cache-write partitions. The adapter now adds all three
with checked arithmetic; an absent partition leaves input unknown. Cached counts remain
subsets, and output continues to include reasoning with reasoning separately reported. This
matches the [pinned vendor implementation's partitioning](https://github.com/anomalyco/opencode/blob/v1.19.0/packages/opencode/src/session/session.ts#L340-L386).

This is confirmed with actual cache-bearing native JSONL, not a fabricated usage fixture. For
example, legacy parallel reports 6,888 fresh plus 7,552 cached = 14,440 normalized input tokens;
its native total is 14,510 including 70 output. Each native projection agrees exactly with the
raw partitions. Successful correction combines both observed inferences once. Persistent
failure retains both native costs while leaving absent logical usage unknown. Unknown tool
tokens and normalized provider-cost counters are never treated as measured zero.

## Actual campaign

`opencode-layout.py` operates public C5 and ProductRunner interfaces through the unmodified
official OpenCode 1.19.0 executable, Zen, `deepseek-v4.1-flash`, on native Linux. The dedicated
existing test credential is read through the normal credential interface. The observer captures
rendered system/stdin parts and native JSONL, relaying native stdout/stderr/status unchanged.
It checks native-tool denial, empty plugin/MCP configuration and global permission denial.
No credential value or isolated vendor auth/config file is copied into evidence.

Twenty-five cases pass, with 31 actual native inferences. Nine additional public daemon route
cases pass. These counts cover distinct cases, not unit tests or a whole-product coverage claim.

| Cases | Independently checked result |
| --- | --- |
| Automatic, required and specific, both layouts | Actual `workspace_read` calls return the exact line and independent SHA-256 of fixture bytes; full current declarations match every callable strict schema |
| None and same-model reselection, both layouts | Exact terminal marker, no calls, correct selected layout |
| Parallel limit two, both layouts | Two actual reads of distinct Unicode fixtures; strict maximum two, required minimum one |
| Unavailable selected tool, both layouts | Rejection before inference; no native submission or host execution |
| Changed host marker, both layouts | Only the developer marker changes; prompt bytes and all other system bytes agree |
| One malformed reply, both layouts | Actual model-authored invalid `tool_calls` type; one correction, one valid host read, complete combined usage |
| Repeated malformed replies, both layouts | Exactly two native inferences and audits; terminal failure; zero host calls; native accounting retained |
| Default layout | Omitted builder selection produces once-only and a real valid read |
| Four ProductRunner chats | Two alternating-order pairs; two exact real reads per task, exact independent JSON answers, unchanged files and identical initial logical views |
| Nine route cases | Zen/Go defaults and explicit selections; malformed value, unknown field and setting on another adapter reject |

The C5 consumer carries the complete 29-tool current host catalog. Cross-layout strict schemas
and system bytes match exactly; the once-only prompt index contains names only. Tool bodies,
argument schemas and descriptions are compared to actual host definitions. Host file outputs
are checked against independently created UTF-8 fixtures and hashes. ProductRunner evidence
also reconciles each native capture to its projection, canonical C5 response, request count and
cumulative diagnostic ledger. Eight ProductRunner model requests/native inferences complete
without correction; no missing input/output/cache/reasoning/total counter is hidden.

Fault-only generations change native instructions and, for repeatable persistent-failure
induction, native stdin to ask the real model for malformed text. Actual instructions/input
and original adapter parts are distinguished. No provider reply, tool result or usage is
substituted. These cases are excluded from size/token savings. One initial persistent-fault
induction instead produced a valid corrected read; it is retained as
`fault-induction-not-malformed-legacy`, excluded from passing persistent-failure evidence and
performance samples. A fresh actual two-malformed-turn case then exercised bounded failure.
The first observer revision is retained as `i3-original-layout-observer.py`; later captures also
record their observer SHA-256. Checker-only fixes for native-outcome field names and array-form
view hashes rechecked completed evidence without resubmitting its native attempts.

## Measured effect

Both chat pairs use identical scope/model/settings, fixed independently generated answers and
fresh private local state. Order alternates legacy/once-only, then once-only/legacy. Shared
vendor caches are outside the experiment's control and reported separately.

| Both paired tasks combined | Legacy | Once-only |
| --- | ---: | ---: |
| Native/model requests | 4 | 4 |
| Total input, including cache | 41,780 | 28,529 |
| Cached input subset | 256 | 11,776 |
| Output including reasoning | 823 | 934 |
| Reasoning output subset | 595 | 706 |
| Native total tokens | 42,603 | 29,463 |
| Adapter bytes, all requests | 186,296 | 126,556 |
| Completion median, seconds | 6.810 | 7.023 |

Total input falls **31.7%**, complete input/output tokens **30.8%**, and rendered adapter bytes
**32.1%**. Output increases in this small sample and is included. Pair zero takes 6.781 versus
7.305 seconds; reversed pair one takes 6.839 versus 6.740 seconds. The median is 3.1% slower;
there is no reliable provider-latency improvement claim. First native/tool-proposal turn times
are 2.612/2.512 seconds and 2.298/2.351 seconds. Whole-operation time to first useful action is
not separately measured in these chats. Top-level host execution totals are 5.973/6.071 ms and
4.784/6.154 ms; I3 does not optimize filesystem/checkpoint work.

The direct automatic control retains all 29 callable definitions while shrinking the adapter
from 66,548 to 38,869 bytes (41.6%); native total input falls from 14,439 to 8,569. Specific
selection preserves the existing smaller system contract and removes the larger duplicate
prompt catalog. These controls establish deterministic delivery reduction; they are not
alternating latency samples.

The volatile-host experiment preserves the developer origin and placement. The earliest
changed system byte is **1,210** in both layouts, before the strict schema. Thus this increment
does not lengthen that local stable prefix. Vendor cached usage is an independent observation;
neither the prefix nor the reduced catalog proves cache acceptance or a dollar-cost saving.

## Reproduction, identities and coverage

Raw evidence root: `/home/doll/.cache/peritus-efficiency-dif0njrm/candidate/`.
Final campaign: `i3-layout-live/result.json`, `config-results.json`, per-case private captures,
canonical requests, traces, audits, specifications and `verified-operation.json` receipts.
`i3-source-identity.json` binds the focused dirty-source snapshot, Cargo manifests and actual
consumer/prior-reproducer binaries; the Git HEAD alone does not identify these uncommitted edits.
All generated workspaces and binaries remain outside the checkout.

Build and checks:

```sh
cargo build --locked --jobs 2 -p peritus-daemon -p peritus-cli -p peritus-tui
cargo clippy --locked --jobs 2 -p peritus-provider-opencode -p peritus-daemon \
  -p peritus-provider-core --all-targets --all-features -- -D warnings
cargo fmt --all --check
git diff --check
```

All pass. `i3-verified-build.jsonl/.log`, `i3-verified-clippy.log`, `i3-format.log`,
`i3-diff-check.log` and `i3-verified-link.log` retain the commands' evidence. The final rebuilt
consumers are byte-identical to those used by the live campaign:

| Artifact | SHA-256 |
| --- | --- |
| C5 layout consumer | `95623de990063d16a54099a568fd78a0c2c11b8f7999e35a3214eba743830997` |
| ProductRunner consumer | `965bcdb3adbd5485becff8193ec74fbaf6cb520f3ca2281186966841e27e30f3` |
| Official OpenCode executable | `135ac3f86c43f2a22328a3554fec5f0bc57d71576077902661bc38dac34871fd` |
| Final observer | `9a11281815e1a1d42ae348d99d316faaddf68b8ab8c3b0da3d868bfa3512dd49` |
| Initial observer | `91d372df89483134694d8d5ad8c936f1e080ee317fbaf5ac4851bda0b9f48211` |

See the [reproducer instructions](reproducers/incremental-context/README.md#i3-native-provider-layout)
for fresh linking and replay. `--resume` revalidates completed receipts with matching binaries
and refuses resubmission when captures exist without a completed result. The retained generator
uses seed 823915, two task pairs, asymmetric independent answers and Unicode file contents.

Source coverage traces request rendering/selection and its constructor/model-selection callers,
daemon route declaration/registry wiring, native isolation/process invocation, JSONL and terminal
decoding, correction/usage combination, ProductRunner tool execution, trace and diagnostic
accounting. This is a focused I3 coverage record, not a whole-codebase audit. Native Go, Windows,
other models, managed edits and the UI were not requalified in this increment. Schema equality
does not establish that every tool has been run. Those limits do not become passing samples.
Prior I0–I2 evidence and later-stage gaps retain their stated scope.

Branch `develop` and HEAD `d58ccbed426fba0a361a6533bb4ddb80726ba2ce` remain unchanged. Existing
unrelated work is retained. No user service/configuration, credential, commit, push or deployment
was changed. I4–I6 are not resumed or declared complete by this phase.
