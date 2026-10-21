# Route-specific model capacities — real testing, 2026-10-21

The generated hosted 32,768-input / 4,096-output profile was still controlling the developer
loop even after OpenCode model discovery supplied larger capacities. The fix resolves numeric
limits for the exact selected service/model before context assembly and negotiation. It preserves
the configured route, capabilities, lifecycle guarantees, and non-token limits.

An advertised shared context window is distinct from separate input and output maxima. Uncapped
turns now select input against the shared window with minimum reply space, then fit generation
output to the remaining window and model output maximum. Explicit output allowances reserve only
the requested amount. This avoids reserving a 384K maximum output on every request, and also handles
models whose output maximum equals their shared context window.

The same resolution reaches local memory, legacy compaction, reviewer/fixer evidence projection,
request negotiation, output generation, effort wrappers, and model switches. OpenCode selection
copies the existing catalog; ordinary turns do not repeatedly rediscover it. Hosted HTTP discovery
is prepared once, reuses an already populated catalog, and retains fallbacks on discovery failure.
Metadata preparation errors stay on the writer/fixer role recovery path.

Generated routes explicitly enable discovery. Existing generated 32K/4K profiles migrate to it
without editing installed settings. Custom pairs remain fixed unless explicitly enabled;
`discover_model_limits = false` preserves even a deliberate 32K/4K pair. Compatible endpoints
and account providers keep their configured behavior.

## Build and provenance

Base: develop at `559dc98293e0875c537543dd47969d927b41b12f`, with this working diff. Rust
`1.97.1 (8bab26f4f 2026-07-14)`. Build used one Cargo job, no debug information, and cached
locked dependencies. No unit tests, lint suites, or repository verification gates were run.

```sh
env CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_DEV_OPT_LEVEL=0 \
  CARGO_TARGET_DIR=/absolute/path/to/cargo-target \
  cargo build --offline --locked -p peritus-daemon -p peritus-launcher --message-format=json
python3 docs/testing/reproducers/model-capacities/build.py \
  --target /absolute/path/to/cargo-target --output /absolute/path/to/new-output
python3 docs/testing/reproducers/model-capacities/replay.py \
  --consumer /absolute/path/to/new-output/consumer \
  --output /absolute/path/to/new-campaign --key-file /absolute/path/to/private/opencode.key
```

The ordinary Rust consumer links the actual Cargo cohort and calls the production provider
registry, DeveloperLoop, LocalContextHandle, and workspace tool executor. Its observation wrapper
records numeric request metadata and forwards the actual provider operations unchanged. Official
OpenCode 1.19.0 performs Zen inference and Zen/Go discovery; the Codex account smoke uses the actual
signed-in executable. The HTTP catalog check uses the actual public OpenRouter models endpoint with
an inert, non-entitled credential placeholder; no user key is sent to that endpoint.

[Compact evidence](model-capacities-evidence.json) includes executable hashes, actual catalog
values, request limits, and independent durable-checkpoint readbacks. Full disposable evidence is
outside the repository at `/home/doll/.cache/peritus-model-limits-20261021/headroom-final-campaign`.
The private key is neither retained here nor included in model prompts or command arguments.

## Observable contract and results

Expectations were established before each final replay: honor exact route/model metadata; preserve
explicit settings; distinguish shared context from independent ceilings; leave non-token limits
and account-provider settings intact; read actual fixture files and return their exact contents.

| Scenario | Observed final behavior | Result |
| --- | --- | --- |
| Public capacity API: 10 valid metadata combinations | Separate input/output fields, missing-field fallbacks, smaller route limits, and shared context resolve to independently specified values. Tool, parallel-call, media, identity, and capability fields remain unchanged. | PASS |
| Four invalid metadata/request combinations | Zero limits or explicit output consuming the shared window are rejected. | PASS |
| Seven shared-output boundary checks | At 900K input in a 1M shared window, uncapped output becomes 100K; at 999,999 input it becomes one token; full input is rejected. Equal context/output maxima work. Explicit 2K output stays 2K. | PASS |
| Real hosted HTTP catalog | Six actual models resolve their own capacities and remaining output space through HostedProvider and model selection. | PASS |
| Real Zen and Go catalogs | Both advertise 1,000,000 shared context and 384,000 maximum output for `deepseek-v4.1-flash`. Uncapped input target is 999,999; explicit 2,048 output gives 997,952 input. | PASS |
| Six explicit-policy combinations across Zen/Go | Forced fixed defaults stay 32K/4K; custom unflagged 70K/9K settings stay fixed; explicit discovery replaces that custom pair with model metadata. | PASS |
| Twelve model switches across Zen/Go | Selected adapters reuse their catalog and resolve the newly selected model rather than inheriting the prior model's capacities. | PASS |
| Fixed-profile comparison, real DeepSeek inference | Three requests retain 32K/4K negotiation and exact fixture-file readback. One history compaction is observed. | PASS |
| Automatic profile, real DeepSeek inference | Three requests estimate 82,162–82,746 input tokens, negotiate 999,999 input / 384,000 output, read actual files, and return the exact generated answer. Zero compactions. Durable memory independently records 999,999 capacity and zero compactor failures. | PASS |
| Actual Codex account workflow | Three requests retain configured 234,567 input / 16,000 output; real workspace reads return the exact generated answer. Durable memory records the same input capacity. | PASS |

The final campaign made nine actual inference requests and used seed `271828`. Large-task fixtures
contain 2,400 seeded hexadecimal records, about 213 KiB; expected answers are fresh values stored
only in real fixture files. Input figures above are Peritus estimates, not provider-billed tokens.

During exploration, the first fixed-profile checker incorrectly expected oversized literal task
pins to be rejected. Actual local-memory behavior preserves those pins beyond its soft selection
target, while compacting optional history. The final check correctly measures negotiated settings
and observed compaction; the program was not changed to reject those tasks. An initial Codex exact
answer included a tool display line number. The replay instruction was clarified to distinguish
line-number framing from file contents; the final exact-content check remains unchanged and passes.

## Coverage boundary

This exercises real provider discovery, registry configuration, model selection, durable local
context, request construction, tool dispatch, and inference. It does not exercise a full 1M-token
inference, generate 384K output, or establish Go subscription entitlement. Go coverage is real
metadata/selection/policy operation; inference used the supplied Zen key. Hosted HTTP inference,
UI onboarding, and unrelated providers were not live-tested. OpenCode output allowances remain
advisory because its runtime exposes no exact output-token control.

Only disposable consumer/vendor processes were launched. Installed binaries, saved credentials,
and ongoing user runs were not changed.
