# Interactive design replay

Requires the real ChatGPT-account provider already authenticated through its official executable, a current matching `peritus`/`peritusd` binary pair, and `uv`. This spends real provider requests. It uses an ordinary disposable Python CLI and isolated XDG state, without changing HOME, CODEX_HOME, the personal Peritus daemon, or provider credentials.

```sh
uv run --with pyte==0.8.2 --no-project python docs/testing/reproducers/guided-design/replay.py \
  --binary /absolute/path/to/peritus --output /absolute/new/disposable-directory --seed 20261015
```

The seed selects a replacement flag and literal Unicode acceptance cases. Add `--build` to commission and observe actual implementation, then execute the resulting CLI against independent literal expected outputs. Without that flag, the journey stops after saving the approved design. Add `--read-only` to admit the fixture as read-only; drafting should still save its own conversation artifact, and project files must remain unchanged. Build is intentionally unavailable in that replay configuration.

`--resume-after-show` resumes the same owned replay directory after its first three completed steps, preserving their evidence; use the same seed and binary. It was used to continue after correcting the original formatting-only Show oracle. The driver handles the real terminal's lazy authenticated conversation binding by confirming no input was durably submitted before following its “Send again” instruction.

Expected behavior: a focused interview with suggested answers; saved Markdown with the requested values; corrections replacing prior choices; full draft readback; exact saved bytes surviving client close and daemon restart; approval preserving project files; explicit implementation receiving the saved design and honoring the corrected flag. The driver records actual terminal screens, command inputs, final state, and draft content. It never substitutes provider replies, tools, pages, or program behavior.

For the browser journey, launch the built `peritus-web` with the same disposable XDG roots and project, then operate **Design something**, **Revise draft**, **Show draft**, and **Start implementation**. Use an independently registered empty folder for a separate greenfield project, or wait for another conversation in the same workspace to become idle before sending a message: the existing workspace execution admission is exclusive. Change a requirement, reopen the tab, and inspect the actual saved draft. **Focus conversation** must enlarge the reading area; reloading must retain the choice; **Show workspace panels** must restore the explorer, project navigation, and controls. Inspect the real DOM dimensions and screen, not a substitute page.
