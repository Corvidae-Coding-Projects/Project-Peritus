# Open issue replays

Real application interface consumers for the October 22 issue/PR campaign.

| File | Role |
| --- | --- |
| [readiness.rs](readiness.rs) | Negotiated IPC admission rejection and fresh metadata mutation/readback against the real daemon. |
| [readiness.py](readiness.py) | Isolated real-daemon lifecycle and actual native TUI failure, retry, and recovery controls; deliberately absent OS credential store. |
| [ui.rs](ui.rs) | Actual TUI runtime with public bootstrap facts supplied directly, without account-onboarding prompts. |
| [link.py](link.py) | Public-consumer linking against the exact artifacts from a Cargo application build. |
| [healing.rs](healing.rs) | Actual public JSON-healing API, generated C0/Unicode preservation, recorded trailing-fragment repair, and strict rejection boundaries. |
| [README.md](README.md) | Execution instructions and scope boundaries. |
