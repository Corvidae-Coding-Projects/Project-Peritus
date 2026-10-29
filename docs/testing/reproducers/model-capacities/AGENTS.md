# Model capacity replays

## Responsibility

- [consumer.rs](consumer.rs) operates production provider discovery, numeric capacity resolution, and the
  developer loop with durable memory and real workspace reads. Its provider wrapper records
  requests and delegates inference unchanged to the actual OpenCode adapter.
- [build.py](build.py) builds/links that consumer using a single Cargo artifact cohort and one build job.
- [replay.py](replay.py) creates fresh seeded fixtures and operates the actual catalogs and inference routes.
- `README.md` describes credentials, disposable fixtures, replay modes, and evidence boundaries.

Never commit keys or large successful inference transcripts. Catalog listing is metadata evidence,
not entitlement or proof that a million-token inference was executed.
## Working in this area

Start with [`README.md`](README.md) to recover prerequisites, disposable-state rules, and the independent oracle before running anything. Keep generated binaries and mutable run state outside the repository, and treat a replay as evidence for the exact observed build and scenario rather than universal product coverage.
