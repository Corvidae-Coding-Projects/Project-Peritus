# Market and research public-library replay

`market-research-consumer.rs` operates the actual public `peritus_modules::Catalog`
and independently compiled `peritus_deep_research::Research` APIs from a minimal
executable. Deep Research implementation is owned by the module-market repository,
not the Peritus binary. This consumer does not use Rust's
unit-test harness. Research observations supplied by this consumer are explicit
host API inputs: they do not establish HTTP, provider, or UI behavior. The
full application campaign supplies that separate evidence.

The oracle follows the user requirements: gathering waits for exact approved
plans; unresolved clarification and stale Start fail; quotations must match
actual role-specific observed byte ranges; discovery-only material stays
excluded; publisher claims require qualification; synthesis sees admitted
evidence; every question receives a disposition; final approval binds the
actual inspected report and every independently reread passage. Revising a
report invalidates approval. Committed research survives a fresh process;
damaged immutable report bytes cannot be presented as checked research.

Run against a fresh disposable directory. Example setup from repository root
(the temporary crate remains outside the workspace):

```sh
mkdir -p /tmp/peritus-market-consumer-build/src
cp Cargo.lock /tmp/peritus-market-consumer-build/Cargo.lock
cat > /tmp/peritus-market-consumer-build/Cargo.toml <<EOF
[package]
name = "peritus-market-research-consumer"
version = "0.1.0"
edition = "2024"

[dependencies]
peritus-modules = { path = "$PWD/crates/orchestration/peritus-modules" }
peritus-deep-research = { path = "/home/doll/Peritus-Module-Market/modules/deep-research" }
serde_json = "1"
toml = "1"

[profile.release]
debug = 0
EOF
ln -s "$PWD/docs/testing/reproducers/market-research-consumer.rs" \
  /tmp/peritus-market-consumer-build/src/main.rs
cargo +1.97.1 build --release --jobs 2 \
  --manifest-path /tmp/peritus-market-consumer-build/Cargo.toml \
  --target-dir "$PWD/target"
target/release/peritus-market-research-consumer \
  /tmp/peritus-market-research-new-run
```

Do not start this Cargo build concurrently with application builds on a
resource-constrained workstation. The consumer starts 24 short sequential
fresh-process report readbacks, three maximal-page readback processes, and no
persistent background service. It retains
`receipt.json`, exact expected reports and per-journey research directories in
the supplied disposable output root. Use a new root for each run.

Generation is deterministic: 24 complete research journeys vary Unicode
prefixes, absolute byte ranges, evidence identities, support/contradiction,
qualified first-party claims, excluded leads, two/three-question plans,
unknowns, rejected drafts, corrections and version-bound rereads. Twelve
catalog sizes cover independent packages, Unicode labels, invalid identities,
unsafe paths, malformed versions/digests, control characters, duplicates and
unexpected executable fields. The checker uses expected transitions and exact
fixture bytes; application output is never adopted as its oracle.

Three legal maximal web-page shapes (about 1 MiB of ASCII, JSON escape-heavy
text, and multibyte Unicode) must persist as host observations and admit an
exact 16 KiB quote spanning a storage chunk boundary after fresh-process
reopen. This checks the domain persistence boundary, with explicit injected
host observations rather than a fabricated claim of HTTP execution.

Two source-rooted regression scenarios have explicit replays:

* `host-observation-before-start`: saving a valid pending plan and calling
  `observe("gather", ...)` must refuse before exact human approval. Earlier
  domain source admitted that preapproval observation.
* `review-must-read-current-actual-report` and `changed-report-must-be-read`:
  passage observations and knowledge of a report digest alone cannot approve
  a report. Fact-check must inspect the actual current report; changing the
  report requires a new inspection receipt. Earlier domain source did not
  record or require this inspection.
* `maximal-N/legal-page-recorded`: a legal 1 MiB web observation must remain
  usable by the research ledger. The original domain rejected all three
  maximal page shapes because serializing page text and metadata exceeded its
  per-record 1 MiB bound. The retained driver then checks exact quotations
  across the proposed storage chunk boundary in a fresh process.

These regression expectations were derived independently before running.
The first two were identified in source and executed on the corrected domain;
no pre-fix executable run is claimed for those two. The maximal-page mismatch
was reproduced on the original executable consumer before its domain fix.

The final Linux replay of the corrected domain completed all 24 research
journeys, 12 catalog sizes and three maximal-page cases with no unresolved
expectation mismatch. All 27 sequential fresh-process readbacks succeeded.

After moving Deep Research into its independent native module, the same
consumer was rebuilt against `peritus_deep_research::Research` in the module
market and `peritus_modules::Catalog` in Peritus. The final native-library
receipt is `/tmp/peritus-market-research-native-library-final/receipt.json`;
all 24 research journeys, 12 catalog sizes and three maximal-page scenarios
passed again.

## Actual native executable IPC

`native-research-rpc.py` starts the separately compiled native Rust executable
and exchanges real four-byte length-prefixed SDK envelopes through its actual
stdin/stdout interface. It exercises an exact identity handshake, rejected
versions, stale plan Start, revision/role keys, independently read source
passages, claim-specific admission, excluded discovery material, qualified
publisher claims, full question coverage, substantive correction routing,
fresh report inspection and final approval. Three complete workflows vary
1 MiB ASCII, escape-heavy and Unicode page inputs. Each workflow shuts down
and starts a fresh native process, reads the same checked report, then opens
the actual module-produced Design attachment and compares its exact bytes.

```sh
python3 docs/testing/reproducers/native-research-rpc.py \
  target/release/peritus-deep-research \
  /tmp/peritus-deep-research-native-rpc-new-run
```

The native executable must first be built from the independent module-market
repository, not a Peritus crate. The delivery replay receipt is
`/tmp/peritus-deep-research-native-rpc-delivery/receipt.json`; all three native
workflow journeys passed. `events.json` records actual method/key outcomes;
per-process stderr files remain beside it. The driver owns and reaps every
native process it starts. Its supplied host observation and model-result
values are explicit API fixtures, not claims of live inference or HTTP
retrieval. No unit-test harness, mock plugin, provider substitute or fabricated
web page is used.

The first native driver run searched for an unprefixed multiline quotation in
Markdown and incorrectly reported two mismatches. Markdown quotations prefix
each line with `> `; the corrected oracle checks that rendering and separately
opens the saved candidate JSON to prove exact quotation bytes and offsets.
That was a checker error, not a product defect.

## Independent extension template proof

A separate temporary Rust crate copied the market's
`templates/rust-plugin/src/main.rs` unchanged and replaced only inherited Cargo
metadata/dependency paths with explicit package values. It compiled as
`peritus-example-counter` version `1.0.0`, using the market SDK, without making
the feature a Peritus crate. Its native `example.counter` package was installed
and enabled through the actual Peritus CLI using a disposable `XDG_DATA_HOME`.
CLI readback verified the declared `x86_64-unknown-linux-gnu` artifact and exact
installed executable bytes.

An actual framed IPC consumer compared both the direct `count TEXT` command
and exported `text_count` function against independently computed Python
`len(text)`, `len(text.split())` and `len(text.encode('utf-8'))` for 12 varied
valid inputs: empty, ASCII, quotes/slashes, multiline, tabs/CRLF, composed and
combining accents, emoji, CJK, Arabic/Hebrew and Unicode whitespace. Wrong
version, invocation before initialization, unknown functions,
malformed arguments and subsequent healthy recovery were exercised. This
does not claim provider inference; the application campaign supplies that
separate wiring proof.

The real CLI initially rejected the standard `x86_64` target because its target
validator excluded underscores. The saved pre-fix command exited 70; the fixed
CLI installs the same untouched template package successfully. Disposable
evidence is retained under `/tmp/peritus-counter-extension-acceptance/`:

* `crate/`, `package/`, `rpc.py`: independent build and direct native consumer.
* `build.log`, `rpc-output/receipt.json`: actual build and 12-input IPC results.
* `manifest-underscore-failure.json`: exact original CLI command/exit/stderr.
* `corrected-install.json`: install disabled, explicit enablement, loaded target
  triple, package digest, registry listing and byte-identical executable.

Observed package digest:
`b6cbcb9c6bc4dd519fe1ba488bc5cd23f73b817073cc4125a90dd187a30d9094`.
Observed executable SHA-256:
`06a952022979b45eacd666710d7d7d280310e0aff4a5b200cf31c6cd9e75a4ad`.
