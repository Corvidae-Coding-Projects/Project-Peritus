# Route-specific model capacities

This is an ordinary public-library consumer, using the production provider registry,
DeveloperLoop, durable local context, and actual tool executor. Inference uses the installed
official OpenCode executable and a real Zen credential. It does not substitute model replies.

Build and link the consumer, with one Cargo job (the account smoke also needs an installed, signed-in official Codex executable):

```sh
python3 docs/testing/reproducers/model-capacities/build.py \
  --target /absolute/path/to/cargo-target --output /absolute/path/to/new-output
```

`consumer checks` exercises the public capacity-resolution API against independently specified
metadata, including partial fields, shared context, asymmetric ceilings, invalid values, and
preservation of non-token limits and explicitly configured snapshots. `consumer catalog opencode-zen`
and `consumer catalog opencode-go` discover actual route catalogs without inference. Supply the
private key file through `PERITUS_REAL_KEY_FILE`; never put its contents in command arguments.
Catalog listing establishes metadata, not Go subscription entitlement.

For `consumer run DIR`, create a disposable directory with `task.txt` containing at least 200 KiB
of inert fixture context plus a request to read `answer.txt`, and `answer.txt` containing a unique
expected answer. The consumer runs the actual developer loop with durable context and real
workspace tools. It requires an actual request above the old 32K estimate, larger negotiated
limits, a completed tool call, and the exact answer. It records numeric request metadata only.
`consumer fixed DIR` uses the explicit old 32K fixed profile and checks its negotiated limits
remain 32K/4K. That value is a context selection target; local memory preserves literal task pins
even when they exceed it. `consumer codex DIR` exercises an actual account-provider workspace
read and exact-answer workflow using the selected model's fresh advertised capacities. Use fresh directories.

See the campaign report for commands, fixture generation, observed values, and limits of coverage.

Run the complete bounded campaign with fresh state and generated file contents:

```sh
python3 docs/testing/reproducers/model-capacities/replay.py \
  --consumer /absolute/path/to/new-output/consumer \
  --output /absolute/path/to/new-campaign \
  --key-file /absolute/path/to/private/opencode.key
```

The fixed and automatic profiles use the same seeded 213 KiB context shape. The account smoke
uses a small task. The actual expected answers are fresh fixture-file values, absent from prompts.
Shared-window API cases reserve the explicit reply allowance, or the advertised output maximum
for an uncapped caller, and check that generation shrinks its allowance when assembled input
fills the shared window. During the prepared-context API migration, older expectations that
reserved only one token for uncapped callers were updated to this current reservation contract;
historical campaign records describe the earlier implementation.
