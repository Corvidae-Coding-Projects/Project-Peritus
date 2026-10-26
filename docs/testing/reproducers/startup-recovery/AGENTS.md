# Startup recovery replay

## Responsibility

[launch.py](launch.py) drives the real CLI/TUI and daemon with disposable XDG data. `README.md` explains prerequisites, the operating-system delay, and expected state transitions. It exercises startup waiting, folder identity, renewed trust and settings repair. Keep its oracle based on retained workspace identity, explicit trust and real attachment; never replace the program with a fake daemon. The large private archive remains external to the repository.
## Working in this area

Start with [`README.md`](README.md) to recover prerequisites, disposable-state rules, and the independent oracle before running anything. Keep generated binaries and mutable run state outside the repository, and treat a replay as evidence for the exact observed build and scenario rather than universal product coverage.
