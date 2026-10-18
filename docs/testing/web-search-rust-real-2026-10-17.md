# Built-in Rust web search: real-interface evidence

Verdict: **ready for the exercised Linux paths**. The default search path now works from a fresh Peritus profile without setup, an API key, Python, containers, a search subprocess, a listener or a first-use download. Optional external SearXNG remains available. macOS and Windows execution were not available in this campaign.

## Contract and implementation boundary

The user rejected exposing container/runtime setup to users and selected a Rust implementation. Expectations were written before execution in `/tmp/peritus-search-runtime-20261017/contract.md` and are retained in the zero-setup revision of `.design/native-web-tools.md`.

`peritus-tools-web` calls the MIT-declared metasearch-rs 0.3.1 library in process. That version avoids the later release's unrelated TUI dependency and its incompatible unicode-width pin. The host supplies an eight-second HTTP client deadline, one fan-out at a time per process, cancellable waiting/requests, admission of HTTP(S) source URLs, bounded result count and visible partial/empty-engine diagnostics. There is no change to provider adapters, credentials, permissions or the optional rendering controller. Linux statically includes the upstream dependency's otherwise implicit OpenSSL dependency; final binaries require no shared libssl/libcrypto/libz.

Native search supports its first ranked set and Yahoo/DuckDuckGo/Brave/Startpage selection. Later pages and date/language filter parameters are explicitly rejected; queries can express those constraints. Engine availability and HTML parsers remain external dependencies, not local readiness guarantees. Native page downloads retain their existing 64 MiB bound; upstream metasearch engine HTML uses the HTTP deadline rather than that page-reader byte bound.

Legacy auto-container endpoints select built-in search without starting a container. Cleanup identities remain intact for explicit ownership-checked stop. Explicit external endpoints and the environment override remain supported. `peritus web setup` now selects built-in search without provisioning software.

## Build and identity

Built with `cargo build --release --jobs 2 -p peritus-cli -p peritus-daemon`, Rust 1.97.1, Linux x86-64. No unit tests, mock search implementations, formal verification or repository verification suites were run.

Final executables:

- `target/release/peritus`: SHA-256 `f7736840ee97389907d4c565edd2bc7257dfcc0041936dfc96dd01def7d4c6d6`
- `target/release/peritusd`: SHA-256 `dcfd29230ad2e709cba2d09e1d4e2d75aad5bd209a44403b95d76adca46477ef`

The first build exposed the upstream OpenSSL shared-library dependency. The corrected static build passed after installing missing build-only Perl modules on this Fedora host. These are compiler-host prerequisites, not end-user requirements. `ldd` on the final CLI listed only the Linux loader, libc, libm and libgcc_s. No existing locked package version was removed or upgraded.

## Observed results

| Actual interface and case | Independent expectation | Result |
| --- | --- | --- |
| Fresh CLI profile, nonexistent/empty PATH, three live technical queries | No helper executable or setup; discover and open official Rust Book, Tokio and Python asyncio pages | PASS: all three domains were returned and their pages read; no child PIDs observed |
| New CLI processes repeat queries, including three concurrent cache consumers | Exact short-lived result reuse, with `cached=true` | PASS |
| Six generated engine/order/count combinations, seed 20261017 | Usable HTTP(S) results, unique URLs, nonempty titles, returned count within requested 1/7/30 maximum | PASS: Brave alone, Startpage alone, combined/reversed/repeated engine names, and mixed working/blocked engines |
| Real blocked Yahoo-only query, twice | Error with engine evidence; no success cache; default search remains usable | PASS: redirect-block errors twice, zero query-cache entries, subsequent default search returned results |
| Public library consumer, active and waiting requests | Both cancel promptly, no detached search; later real search works | PASS: cancellation about 154 ms, followed by 10 results |
| Public library invalid inputs | Reject empty query, page 2, unknown engine, native language/date filter | PASS: five distinct inputs rejected |
| Legacy settings with an unavailable managed endpoint, empty PATH | Native selection without helper/service execution; retain legacy ownership metadata | PASS: status uses built-in search and leaves the original settings bytes/state unchanged until explicit setup |
| Unavailable explicit external endpoint, then setup reset | External error remains visible; built-in recovery needs no provisioning | PASS |
| Actual SearXNG 2026.10.17-2a6fc07be fixture, explicit external selection and reset | Existing JSON endpoint returns useful sources; reset selects native | PASS: 10 results, including Rust Book; fixture stopped and removed |
| Actual terminal, daemon, signed-in official Codex provider | Model discovers via `web_search`, opens a returned official source via `web_open`, and answers from it without setup | PASS: retained tool trace shows native search and HTTP read of Rust Book ownership chapter; terminal closes and daemon shuts down cleanly |
| Existing HTTP/source campaign | Preserve encoding, exact paging/find, redirects, failures, source identity, refresh and persistence | PASS: 29 scenarios, 10 generated Unicode documents, largest source 3,148,008 bytes |

Brave and Startpage supplied useful results during this run. Yahoo encountered redirect-based blocking; DuckDuckGo often yielded no parsed results. Both conditions were exposed alongside working results. This evidence establishes bounded behavior here, not universal engine availability or exhaustive Internet coverage.

## Replay and retained evidence

```sh
python3 docs/testing/reproducers/web-search-native.py \
  --binary target/release/peritus --output /tmp/peritus-search-replay-UNIQUE
python3 docs/testing/reproducers/web-tools.py \
  --binary target/release/peritus --output /tmp/peritus-document-replay-UNIQUE
```

`web-search-consumer.rs` is an actual public-library consumer, not a unit suite. Build it with the current release rlib and serde_json rlib in `target/release/deps` (edition 2024, `-C panic=abort -L dependency=target/release/deps`). Invoke it with a disposable store directory for cancellation/invalid/recovery, or add JSON arguments for an exact search. Pass its executable as `--consumer` to the native replay to repeat the six seeded engine/count cases.

```sh
python3 docs/testing/reproducers/web-search-daemon.py \
  --binary target/release/peritus \
  --provider-state /path/to/nonsecret/product-state/state-GENERATION.json \
  --output /tmp/peritus-search-daemon-UNIQUE
```

The daemon driver requires test-host pexpect/pyte and an already authorized provider. It creates isolated XDG directories and a disposable workspace, then shuts down only its exact daemon/socket. Test-driver Python is not a product search dependency.

Raw local artifacts are under `/tmp/peritus-search-runtime-20261017`: `native-live-final/`, `documents-final/`, `cancel.json`, `generated.json`, `failed-error.txt`, `external-real.json`, and `daemon-final/evidence.json`. The optional combined retained-driver replay is in `retained-driver-final/`. The external service was a developer-owned test fixture, not a product prerequisite. All disposable daemons/containers used by this campaign were stopped; live user daemons and installed binaries were untouched.
