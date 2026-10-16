# Retained-state growth: real-operation replays

These command-line consumers call the actual public Peritus implementations. They retain and
read back real files, canonical snapshots, event streams, scope inventories, and copied C0 state.
The native drivers operate the normal launcher, daemon, TUI, and configured provider. There is
no replacement provider, copied feature implementation, unit framework, or formal suite.

Build consumers outside this checkout, one build at a time:

```sh
python3 docs/testing/reproducers/retained-state-growth/build_consumer.py /tmp/retained-c6 --kind c6
./target/release/peritus-retained-c6-consumer /tmp/retained-c6-run --final --reuse-original /tmp/retained-c6/original-inputs
python3 docs/testing/reproducers/retained-state-growth/build_consumer.py /tmp/retained-protocol --kind protocol
./target/release/peritus-retained-protocol-consumer /tmp/retained-protocol-run --final
python3 docs/testing/reproducers/retained-state-growth/build_consumer.py /tmp/retained-scope --kind scope
python3 docs/testing/reproducers/retained-state-growth/scope_growth.py --consumer ./target/release/peritus-retained-scope-consumer --output /tmp/retained-scope-run --final
python3 docs/testing/reproducers/in-place-scope/replay.py --consumer ./target/release/peritus-retained-scope-consumer --output /tmp/retained-scope-sequences
```

Supply fresh output paths. The compressed original v1 inputs are anonymous generated data;
`fixtures/manifest.json` binds their exact uncompressed bytes. The C6 replay reuses those inputs,
checks policy-only migration, crosses seven former boundaries, persists/reopens exact state,
and checks page completeness, ordering, role isolation, corruption rejection, and failed-decoder
poisoning. The protocol replay crosses ten former boundaries and checks real canonical
encode/decode and successful reducer terminals. The scope replay creates 5,000 files, 66 nested
directories, and 67,010,560 bytes of preview-bearing source, then uses fresh consumers to check
changes, restoration, and unrelated-file preservation. The existing scope sequence driver adds
brownfield edits, legacy imports, overlapping declarations, links, private files and runtime churn.

For host recovery, build `--kind host`. Use the adjacent `../token-budgets/prepare_host.py` on a
**closed backup** with a separate disposable workspace copy. `prepare_trace.py` takes an owned,
closed native fixture, that host-setup script, a fresh output directory, and the completed run ID.
It adds one valid historical metadata frame with more than 1 GiB of physical JSON whitespace;
it validates every added byte in bounded chunks. Run the host consumer on the resulting directory,
then run it again in a fresh process. It reconstructs production context, publishes a schema-3
checkpoint, reads it through the actual inspector, and checks exact retained entries and sources
without inference or tool dispatch. `--load-only` isolates recovery and framing.

`prepare_native.py` and `native_replay.py` retain a concrete completed SOURCE-REPAIRED fixture
from the binary-evidence campaign. They copy its closed state/config, pad only its valid metadata
with 128 MiB of whitespace, and require the exact completed run through both CLI and visible TUI.
They do not retry that run or request inference. Python `pyte` is required for native drivers.

`prepare_memory_chat.py` creates a new nongit project and private XDG roots from an owned fixture's
provider configuration. It creates forty real source files and an exact 33-operation memory
proposal, including Unicode above 2 KiB and forty source references. `memory_chat_replay.py`
requires an explicit `--run-provider` flag and uses the real configured provider through the TUI.
The oracle checks actual read provenance, accepted update bytes, complete 2-KiB cursor pages,
JSON fragments, and unchanged project hashes. Its `--seconds` option bounds this disposable
campaign, not the Peritus run engine. Stop only the driver-owned client/daemon; never point these
scripts at live user state. Credential/config contents and real job transcripts are not fixtures.
