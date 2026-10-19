# TUI convenience replay

Build the current CLI/TUI, then run this driver with an absolute binary path and a compatible
packaged daemon. It copies the executables into disposable state; no live run is opened.

```sh
CARGO_TARGET_DIR=/home/doll/.cache/peritus-tui-feedback-target CARGO_PROFILE_DEV_DEBUG=0 \
  cargo build --locked --package peritus-cli --package peritus-tui --jobs 1
uv run --offline --with pyte --with wcwidth python \
  docs/testing/reproducers/tui-conveniences/replay.py \
  --binary /home/doll/.cache/peritus-tui-feedback-target/debug/peritus \
  --daemon /home/doll/.local/bin/peritusd \
  --output /tmp/peritus-tui-conveniences-new --provider
```

Requires a real Xvfb and xclip for its isolated clipboard and pyte for reading terminal frames.
Provider mode requires a ready real OpenAI account and performs a read-only conversation.
The fixture supplies a small direct file editor through VISUAL, including an executable path
with spaces. The editor operates the real temporary draft passed by the application.
It is not an alternate UI implementation. The independent oracle checks the unsent result.

`--reuse-runtime PATH` can reuse a previous disposable fixture's data/browser installation to
avoid downloading the identical browser automation runtime again. It does not reuse daemon state.
Never point `--output` at a user project or existing state directory.

`archive-replay.py --deps <built debug/deps> --output <new directory>` builds a small consumer
of the current production transcript module, writes 300 varied activities, and reopens them from
separate processes with real ripgrep and with ripgrep unavailable. `archive-consumer.rs` contains
independent expectations for literal/Unicode queries, byte fidelity, streaming replacements,
permissions, isolation, and truthful bounded result pages. This supplements the actual TUI search
journey rather than replacing it.

`--local-only --provider` keeps a real provider configured for local attachment validation, but
skips creating a provider response. `--resume-response <owned fixture>` reopens that fixture's
retained run with the current binaries and checks copying, colors, search, generated draft
workflows, and restart without another model request. It requires the explicit owned-fixture marker.

`attachments.py` uses the same fixture/PTY definitions, then performs the final image and file
confirmation and retained-list readback after reopening an owned run. Pass the same binary,
daemon, output, `--provider`, `--reuse-runtime`, and `--resume-response` arguments. It validates the
isolated clipboard service before pasting and checks that confirmation keeps the main draft and
does not initiate inference. This retains the regression for an image preview formerly lost behind
a missing post-reconnect workbench snapshot.
