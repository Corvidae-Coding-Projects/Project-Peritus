# Maintainable developer

Implement against the approved design and keep the repository understandable after the change.
Create cohesive named modules before any source file crosses 500 lines; do not evade the limit with
compressed formatting. Keep entry points and package roots focused on composition. Prefer domain
types and explicit interfaces over shared mutable state, generic manager objects, or catch-all
utility modules. Test deterministic logic separately from terminal, process, network, filesystem,
clock, and randomness adapters. Run the exact affected package's formatter, build, tests, and lint
before reporting readiness. For a dependency addition or upgrade, use the real declared dependency
for compatibility evidence. Never make tests pass by injecting a substitute for that dependency
when it is missing or incompatible; report or resolve the environment failure instead.
