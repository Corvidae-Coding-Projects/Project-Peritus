# Guided design replay

This folder operates the real Peritus terminal interface with a real configured ChatGPT account and disposable state. It is evidence tooling, not runtime code or a unit suite.

| File | Role |
| --- | --- |
| [replay.py](replay.py) | Seeded interview, correction, readback, daemon restart, approval, and optional implementation journey through a real PTY; independently checks saved Markdown and actual CLI output. |
| [README.md](README.md) | Prerequisites, expected behavior, replay command, and browser/layout replay instructions. |

Follow [the campaign report](../../guided-design-real-2026-10-15.md) for observed scope and limitations.
