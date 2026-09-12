# Local fuzz pilot

- Source: `c42327c1fcd27ba2e8f8b27da541d94923b7c89f`
- Host: Linux x86_64
- Toolchain: `nightly-2026-08-09`, cargo-fuzz 0.13.2, libfuzzer-sys 0.4.13
- Seed: 881
- Per target: 120-second requested budget, 8192-byte input ceiling, 2048 MiB RSS limit,
  30-second input timeout, two compiler jobs

| Target | Engine seconds | Executions | Result |
| --- | ---: | ---: | --- |
| `sse` | 121 | 1,066,818 | completed |
| `ndjson` | 121 | 1,965,813 | completed |
| `working_state` | 121 | 322,628 | completed |
| `provider_sequence` | 121 | 1,172,957 | completed |

Every command ran through the bounded `xtask` process-group wrapper. Each completion record has
exit code zero and reports `kill_requested_and_root_reaped`; no crash artifact was accepted. A
stable replay at the same source revision replayed all four checked-in corpora successfully with
the production Rust toolchain.

AddressSanitizer remained enabled. LeakSanitizer was disabled because it cannot operate when the
campaign process is traced by the sandbox: two otherwise successful pre-fix campaigns completed
more than one million iterations and then aborted during LeakSanitizer finalization. Those attempts
were rejected rather than counted. The semantic oracles, memory-safety sanitizer coverage, process limits,
and crash-artifact checks remained active in the accepted runs.

Inputs were synthetic and copied to private campaign directories. No provider credential, live
network service, external account, unrelated process, or user data was used. Generated corpus and
logs remain under `target/discovery/`; only minimized regressions and this bounded summary are
checked in.
