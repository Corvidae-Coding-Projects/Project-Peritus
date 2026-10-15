# OpenCode malformed model replies

This replay uses the production provider registry, actual official OpenCode 1.19.0 executable,
and a real entitled Zen/Go key. The supplied application configuration must contain an OpenCode
route. Use disposable configuration from ordinary `peritus providers` setup; the consumer uses
the explicitly selected key file and never changes saved settings. An API key is not included.

```sh
python3 docs/testing/reproducers/opencode-malformed-output/replay.py \
  --config /absolute/path/to/disposable/peritus-configuration.toml \
  --key-file /absolute/path/to/private/opencode.key \
  --model deepseek-v4.1-flash \
  --output /absolute/path/to/new-owned-output
```

The output directory must not exist. The runner builds a real public-library consumer with
two Cargo jobs using the checkout's cached dependency versions. It consumes provider usage.
Choose `--model gpt-6-luna` for the second tested model; `--service go` requires Go entitlement.
`--cancel` exercises cancellation and a subsequent ordinary real request instead of the
multi-turn workflows. Successful Go inference was not established with the supplied Zen key.

Each ordinary workflow must read both fixture files through declared host tools and return
a newly generated token learned only from the host result. Generated variants include prose,
empty commentary during tool calls, JSON reports, quoted Windows paths, and Unicode. JSON
reports must parse and preserve exact token/path/label values. Only fully accepted proposals
reach the actual file reader. Native OpenCode operations remain forbidden.

The transparent wrapper relays and saves unchanged vendor JSONL and SDK stderr in the private
output directory. Stderr remains live so vendor-error observation and cancellation can work.
It does not replace responses or simulate inference. Canonical requests,
results, correction events, and provider usage are retained. A correction event must be
observed before claiming recovery-path coverage; clean replies alone establish ordinary
workflow coverage. The sequence is nondeterministic: repeated requests may trigger different
format mistakes. Budget additional attempts only to resolve a specific missing observation.

[observed-invalid-turn.jsonl](observed-invalid-turn.jsonl) preserves the actual baseline
failure: a complete successful vendor execution with an extra closing brace in the model's
private JSON envelope. It is evidence, never injected as a substitute service response.
See [the campaign report](../../opencode-malformed-output-real-2026-10-15.md) for before/after
results and native-platform limitations. No unit tests or repository gates are invoked.
