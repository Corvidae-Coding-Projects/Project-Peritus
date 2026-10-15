# Readline editor real replay

## Folder guide

Real-interface replay resources for readline; start with the local README/driver and its documented disposable-state prerequisites.

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |
| [regressions.json](regressions.json) | Structured regressions data with fields `issue`, `seed`, `regressions`. |
| [replay.py](replay.py) | Operate actual Peritus via PTY, decode its screen, and replay human editing. |

This driver sends terminal bytes to the actual Peritus client and daemon, decodes real Ratatui screen output with pyte, and compares literal editing outcomes established from issue #126. It also generates structured Unicode and whitespace variants using seed 12620261014. It does not substitute a UI model or daemon and does not run a unit suite.

Build the CLI and daemon normally, with bounded resources. Use a disposable offline fixture with an `owned-fixture.json` marker containing `{"owned_disposable_fixture":true}`, a `repo/` directory and separate `config/`, `state/`, `cache/` roots. Initialize it through `peritus providers` (provider 0), then `peritus open FIXTURE/repo` with those XDG roots. Trust the disposable folder and close its client normally. A real terminal must answer device/cursor queries during startup.

With pyte installed:

```sh
python docs/testing/reproducers/readline/replay.py --binary /path/to/peritus \
  --fixture /path/to/disposable-fixture --output /tmp/readline-legacy
python docs/testing/reproducers/readline/replay.py --binary /path/to/peritus \
  --fixture /path/to/disposable-fixture --output /tmp/readline-enhanced --enhanced
```

The legacy replay sends Ctrl-A/E bytes, the documented Ghostty translation of Cmd-Left/Right. Enhanced key sequences also exercise modified aliases, release events and selection. This proves parser/editor/routing behavior on Linux; native macOS/cmux delivery remains outside this host's evidence. Ctrl-Y kill-buffer support and new selection features were explicitly outside issue #126's required scope. Existing review-feedback Ctrl-B/F actions retain their established specialized meaning.

Keep result JSON, failure screens and raw terminal streams outside Git. The driver closes only its own client. Shut down only the owned fixture daemon through its recorded endpoint after replay; never use live user state.
