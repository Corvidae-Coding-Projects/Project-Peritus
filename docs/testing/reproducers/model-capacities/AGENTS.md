# Model capacity replays

- `consumer.rs` operates production provider discovery, numeric capacity resolution, and the
  developer loop with durable memory and real workspace reads. Its provider wrapper records
  requests and delegates inference unchanged to the actual OpenCode adapter.
- `build.py` builds/links that consumer using a single Cargo artifact cohort and one build job.
- `replay.py` creates fresh seeded fixtures and operates the actual catalogs and inference routes.
- `README.md` describes credentials, disposable fixtures, replay modes, and evidence boundaries.

Never commit keys or large successful inference transcripts. Catalog listing is metadata evidence,
not entitlement or proof that a million-token inference was executed.
