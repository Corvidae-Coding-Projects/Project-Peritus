# Peritus property campaigns

This optional, nested Rust workspace exercises real Peritus library interfaces with independent reference models and reproducible generated histories. It is not a root Cargo workspace member, release dependency, or hosted CI gate. The code under `src/` covers the Stage 1–7 public mechanisms; `owner/` and `support/` exercise test-only owner boundaries, including Stage 8 context flow. Focused regressions also live as ordinary package examples and provider loopback consumers under `docs/testing/reproducers/`.

Run from the repository root with the pinned Rust toolchain and locally cached dependencies. Each `--output` directory must be new. The controller records exact case data, seed, source fingerprint, process outcome, and shrink results in that directory. Exit statuses are 0 for pass, 1 for a behavioral mismatch, and 2 for incomplete setup or execution. Test-only owner hooks are enabled only by `cargo test`.

```sh
python3 discovery/proptest-praxis/campaign.py build --output /tmp/praxis-build-1
python3 discovery/proptest-praxis/campaign.py examples --target framing --output /tmp/praxis-examples-1
python3 discovery/proptest-praxis/campaign.py search --target framing --profile smoke --seed 7319 --output /tmp/praxis-search-1
python3 discovery/proptest-praxis/campaign.py replay --case /path/to/minimized-failure.json --output /tmp/praxis-replay-1
python3 discovery/proptest-praxis/owner.py build --target plans --output /tmp/praxis-owner-build-1
python3 discovery/proptest-praxis/owner.py examples --target plans --receipt /tmp/praxis-owner-build-1/receipt.json --output /tmp/praxis-owner-examples-1
```

`--target all` runs every implemented public target. The `contracts/` directory records each independent oracle and its coverage limits. Exact replay cases are written into local run output and accepted by the `replay` command. The property runner never grants host or model authority: native files, processes, browser and installer checks require their separate real-interface fixtures and disposable state. The suite deliberately does not prove all OS schedules, cryptographic collision resistance, or whole-product behavior.

Generated run logs and calibration archives stay local and are ignored. Confirmed defects are retained as small public regressions and minimal production repairs. This checkout includes only runnable source, fixtures, contract notes, and focused replays.

Stage 8 policy and settlement properties are a separate nested consumer under [stage8-policy/](stage8-policy/README.md), with its own pinned lockfile and replay commands.
