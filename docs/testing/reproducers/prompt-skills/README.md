# Prompt skill real-interface replay

Use a disposable folder and independent XDG roots. Initialize the actual Peritus CLI with a real account provider, explicitly trust the disposable folder, and submit a small standalone Rust task through the real composer. Do not use the live user daemon, workspace, session or credentials as fixture data. Account authentication may be reused through the normal provider mechanism.

The campaign's Rust task changes an existing `main.rs` CLI to prefix its output with `double=`. Expected fresh-process results are `6` -> `double=12\n`, `-7` -> `double=-14\n`, and `abc` -> exit 2 with usage on stderr. rustc compilation and resulting CLI execution are allowed; Cargo, unit tests, Clippy, format checks and the optional helper are excluded. Reading `main.rs` twice with identical arguments deliberately checks that a Rust reliability advisory permits a user-requested repeat and subsequent progress.

For the negative control, use read-only `/plan` in a mixed folder containing `main.rs` and `app.py`. Ask only for the exact `app.py` contents and say Rust work is unrelated. Listing the mixed folder must not by itself activate Rust. For resource checks, request exact bundled `skill_read` pages in read-only mode; reconstruct each resource from its continuation pages and compare its SHA-256 with the imported canonical file. The helper is read as source only, never executed.

`archive_consumer.rs` is a small real consumer of Peritus's public `decode_messages` API. Build it outside the checkout with path dependencies on `peritus-model-protocol` and `serde_json`, then run it against only the disposable run's `*.context` directory and an external output directory. It decodes retained canonical C5 artifacts, rejecting malformed/non-message bytes, and records real role/text views that contain the vitamin marker. It neither creates nor substitutes provider responses.

```sh
/path/to/archive-consumer /path/to/disposable/run.context /tmp/prompt-views
```

Inspect current System messages, not quoted skill text in user/tool observations. Multiple assembled views must retain a single Rust marker per System policy; a read-only role still exposes no writable/process tools. Original inner-executor-only activation produced vitamin cues but no Rust System marker. Forwarding the hook through `MemoryTools` repairs the production local-context path. Routine context assembly is covered; forced transport rejection, every long-horizon reasoning drift, and a controlled performance A/B study are separate from this bounded replay.

Preserve concise failure/replay evidence, not full successful model transcripts or personal data, in the repository. Stop only the fixture clients and daemon after the campaign.

The retained `readonly_tui_replay.py` operates the real Plan/Review composer. Set `PERITUS_REPLAY_FIXTURE`, `PERITUS_BINARY`, `PERITUS_PROMPT_MODE` (plan/review), and optionally `PERITUS_PROMPT_TASK` and `PERITUS_REPLAY_SECONDS`. Its completion flag means only that the conversation returned to Idle; decode the actual canonical policies and audit completed tools before claiming scenario success. It never switches to an effectful mode. The default task is the protected-Rust-path Python negative control.

Final observed read-only cases were:

- Python request protecting `main.rs`: no Rust policy; only `app.py` read.
- Python plan “without Rust code”: no Rust policy; only `app.py` read.
- “Without unit tests, implement a Rust CLI” in Plan: Rust at step 1, only a listing.
- Rust Review with checks excluded: Rust at step 1, all six resources reconstructed exactly, no effects.
- An unnamed-language doubler review: no Rust at steps 1–2; Rust at step 3 after the actual source read.
- Out-of-range offset 999999: read error followed by successful 4096-byte reference pages at 0, 3897 and 7780, then a complete default helper-source read. The helper was never executed.
