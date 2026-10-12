# Provider settings restart and historical process recovery

## Result

The repaired native installation opens from `/home/doll/browser`, connects to the daemon, and receives `PERITUS-CONNECTED` from the configured ChatGPT account through the actual TUI. Saving provider selection `1` and launching `peritus` again replaces the old daemon successfully. Fresh CLI status reports `ready-read-write`, `mutation_ready: true`, and no diagnostic.

The source repair is based on `develop` commit `ab5e0d78e` and was installed locally before being committed. No unit tests, mocks, repository verification suites, or formal verification ran.

## Confirmed defects

### PR-001: valid historical process results were quarantined

- **Reproduction:** start the actual daemon against the existing process registry. Its 609 historical manifests contain V2 terminal results. Every outer manifest checksum and every saved embedded-result checksum matches its original bytes.
- **Expected:** decode supported historical formats, preserve their execution facts, and reconcile completed process ownership without disabling the application.
- **Observed before:** all 609 manifests were quarantined, leaving 609 orphan consumption claims. The daemon entered `ready-read-only` with `PERITUS-STARTUP-RECOVERY-001: 609 indeterminate and 609 quarantined process records require operator reconciliation`.
- **Cause:** `crates/runtime/peritus-process/src/recovery/manifest/codec/terminal_payload.rs` compared a historical result's saved digest against the result re-encoded using the current V4 format. The changed canonical format necessarily produces a different digest.
- **Repair:** authenticate the exact embedded payload bytes before decoding. Normalize the verified decoded representation's digest for future writes, then retain the existing phase, identity, output, and ownership consistency checks.
- **Replay:** a copied registry containing all 609 actual historical manifests and their matching claims becomes writable with the patched daemon. The previous installed binary reproduces the fresh-client connection failure against the same historical dataset. Two generated corruption variants remain quarantined: a damaged outer checksum and a damaged embedded-result digest with a recomputed outer checksum.

### PR-002: a fresh client could not inspect or stop a recovery-mode daemon

- **Reproduction:** with the daemon in recovery mode, save a changed provider selection and run `peritus`. The launcher must stop the daemon using its previous configuration before starting the new one.
- **Expected:** an authenticated owner can inspect recovery status and request orderly shutdown without enabling mutations or requiring a previously saved session ID.
- **Observed before:** `peritus: internal: launch interactive product: could not start packaged daemon: shutdown command failed with status exit status: 10`. A fresh CLI status request also fails with `read daemon frame: early eof`. The daemon log reports `read-only diagnostics require an existing durable session`.
- **Cause:** `crates/app/peritus-daemon/src/session/negotiation.rs` rejected every fresh session while read-only. The launcher creates a fresh shutdown connection, so restart could never progress.
- **Repair:** after the existing OS-principal checks, negotiate a transient diagnostic connection without writing a durable session. `session/connection.rs` restricts it to `DaemonStatus` and `Shutdown` for its entire lifetime, before provider discovery or other request dispatch. Other operations receive the existing `read-only` error.
- **Replay:** both corrupt-record fixtures allow three fresh status connections and a completed shutdown. A retry mutation is rejected in each fixture; their `app_sessions` tables remain empty. Shutdown continues to report unresolved process ownership honestly rather than marking corrupt fixtures clean.

## Real execution evidence

Build:

```sh
CARGO_BUILD_JOBS=2 cargo build --release --locked --bins \
  --package peritus-cli --package peritus-daemon \
  --package peritus-tui --package peritus-sandbox-linux
```

The optimized native build completed in 2m19s. Package assembly used `peritus-package --use-release-artifacts`, followed by the native Linux installer. The four installed executable hashes match the rebuilt artifacts. The package assembler was used only to assemble the installation; no qualification campaign ran.

Bounded replay exercised four actual daemon instances: one previous-binary reproduction, the 609-record restored registry, and two distinct corruption variants. It issued three fresh status requests per instance, rejected one retry mutation per corrupt instance, and observed shutdown completion for the three patched instances. The old reproduction instance was stopped separately as an owned disposable process because its fresh diagnostic connection was rejected.

Local recovery preserved 1,222 original manifest and claim files with SHA-256 readback. After the affected daemon stopped, the 609 checksum-validated historical manifests were restored from quarantine to their original manifest names. Consumption claims, spools, configuration, credentials, workspaces, and conversation storage were retained. The repaired startup has zero quarantined process records.

The live TUI displayed `Ready` and `connected to daemon`, then received the exact independently specified reply `PERITUS-CONNECTED`. This used one normal account-provider request and no application tool calls. A subsequent real `peritus providers` selection of `1` saved successfully, and `peritus` relaunched without the shutdown failure. The applied configuration advanced to `peritus-00000000000000000062.toml`, with a new daemon PID and writable readiness.

Evidence and replay driver: `/home/doll/Peritus-provider-restart-repair-2026-10-11/`. Run `python3 /home/doll/Peritus-provider-restart-repair-2026-10-11/replay.py` to replay the retained local cases. The previous binary used by the original comparison is preserved in `previous-install/bin/peritusd`; the original comparison used it before replacing the local installation. The fixtures contain copies of real historical records, not substituted implementations or mocked process probes.

## Limits

This campaign covers historical process decoding, corruption rejection, recovery-mode fresh connections, provider-settings restart, and one real account-provider response on Linux x86-64. It does not establish behavior on other platforms or restore quarantined conversation/run formats from unrelated pre-release schemas.
