# Stage 7 launcher release identity and account executable discovery

## Source and callers read

- `crates/app/peritus-launcher/src/update.rs`, `update/release.rs`, `update/download.rs`: cached public release identity, startup and explicit update callers, and archive tag/checksum use.
- `crates/app/peritus-launcher/src/app.rs`: startup discovery/offer and explicit update route.
- `crates/app/peritus-launcher/src/provider_setup.rs`, `provider_setup/models.rs`, `provider_setup/reviewer.rs`, `bootstrap/configuration/account.rs`: first-run, repair, model selection, and configuration callers of account discovery.
- `crates/app/peritus-provider-onboarding/src/account.rs`: account discovery, native fallback, status, and provider catalog.
- `crates/model/peritus-provider-openai/src/runtime/executable.rs`, `crates/model/peritus-provider-anthropic/src/runtime/executable.rs`, `crates/model/peritus-provider-core/src/process.rs`: PATH iteration and canonical executable pinning.

## Independent contracts and schedules

`stage7-launcher/release_version.py` models a tag as exactly `v` plus three dot-separated ASCII decimal `u64` components. It compares normalized rendered versions against the private production parser mounted by `src/bin/stage7_release_probe/update.rs`. Fixed cases cover leading zeroes, maximum `u64`, overflow, missing and extra fields, prefix/case errors, signed and Unicode digits, and prerelease suffixes. Seed 7319 supplies 1,024 generated tags. `calibration/stage7-release-version.patch` deliberately permits a fourth field and must be detected by the fixed `v1.2.3.4` case; apply only during controlled calibration, then reverse.

`stage7-launcher/provider_discovery.py` models selection of the first canonical regular executable in PATH with an execute bit. Its fixed schedules place a non-executable file before a valid later candidate, an absent first candidate, and multiple valid candidates. Seed 7319 supplies 256 generated three-directory arrangements for each of Codex and Claude. The probe invokes the public executable and onboarding discovery APIs in a separate process with disposable PATH/HOME directories; it never invokes a real provider or touches user credentials. The executable path and success/error state are independently compared to the filesystem model.

## Observed failure and candidate repair

The original source failed 110 of 522 provider discovery cases. Both provider kinds failed the fixed two-directory case: the first file was regular but not executable; the second was executable. Both the raw provider API and `AccountProvider::discover` returned unavailable, while the model selected the second file. The minimized Codex witness and first failure for each kind are retained in `stage7-launcher/provider-pre-fix/report.json`, with original source hashes. The cause is the early `return Self::pin(candidate)` after only `candidate.is_file()`. The isolated candidate uses `if let Ok(executable) = Self::pin(candidate)` and continues the PATH search on a failed pin in each provider adapter. The rebuilt public APIs pass all 522 cases. No login/status executable was invoked in this campaign.

The original release parser passed all 1,037 fixed/generated tags. Applying `calibration/stage7-release-version.patch` to this isolated worktree made the probe accept `v1.2.3.4`, which the oracle rejected at fixed case 9. The mutation was reversed, the probe rebuilt, and all 1,037 cases passed again. Source identity and exact result are in `stage7-launcher/release-original/`, `release-calibration/`, and `release-restored/`. The production release source is unchanged.

## Replay

From the isolated worktree root, with the discovery Cargo workspace's dependencies available:

```sh
cargo build --manifest-path discovery/proptest-praxis/Cargo.toml --bin stage7_provider_discovery --bin stage7_release_probe
python3 discovery/proptest-praxis/stage7-launcher/provider_discovery.py discovery/proptest-praxis/target/debug/stage7_provider_discovery /tmp/stage7-provider-unique
python3 discovery/proptest-praxis/stage7-launcher/release_version.py discovery/proptest-praxis/target/debug/stage7_release_probe /tmp/stage7-release-unique
```

Use a fresh output directory for each replay. Do not run the deliberate release mutation in a shared checkout.

## Limits

The release probe covers tag parsing/rendering; its source mount has the discovery crate's `CARGO_PKG_VERSION`, so it does not establish the installed launcher's `is_newer` behavior. HTTP release discovery, download/checksum/range handling, native install, real provider login, account status, model discovery, and connection checks are outside this bounded target. These sources were inspected to identify callers and boundaries, not covered by the property runs.

## Strengthening pass after v0.1.18

Executable discovery now covers seven entry types: absent, executable, non-executable,
directory, dangling symlink, executable symlink, and non-executable symlink. It exhausts all
49 ordered pairs and generates zero- through six-entry PATH searches, using Unicode/spaced
directory names. Both APIs must select the first valid canonical target. Each child has a
10-second bound. Minimization deletes entries only while the original mismatch persists;
it no longer substitutes an unrelated hardcoded candidate. Seed 7319 passed all 620 cases
in this pass, with every type represented more than 200 times across provider kinds.
