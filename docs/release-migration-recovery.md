# Release migration, backup, restore, and rollback

This runbook defines the evidence H4 requires for a production upgrade. It is an operational
contract, not a claim that any particular candidate was exercised. Candidate-specific evidence
must record actual paths, digests, timestamps, subject identity, adapter result, and cleanup under
the exact `ReleaseBinding`.

## Authority and protected data

The release operator may replace package-owned binaries, helpers, manifests, and supervisor
definitions. An ordinary upgrade does not gain authority to delete or rewrite operator-owned
configuration, credential stores, durable state, journal history, logs, evidence bundles, or an
unrelated installation. Migration code acts only through the existing C0 migration and G0 shutdown
boundaries; H4 does not create a second state writer.

The package lifecycle and durable schema lifecycle are separate:

- package rollback restores prior executable and supervisor bytes;
- database restore replaces durable state only from an independently verified backup while the
  daemon is stopped; and
- a schema downgrade is never inferred from package rollback. It must be explicitly supported by
  the target migration contract or performed as backup restore.

## Evidence bundle before mutation

Before stopping the current daemon, retain a pre-migration inventory containing:

- installed release version and exact executable/helper hashes;
- configured state-root and component-root identities without secret values;
- journal head, store identity, schema version, projection checkpoint, and artifact-store integrity
  summary;
- supervisor definition and package manifest hashes;
- available filesystem space and the selected resource bound;
- active-task quiescence or the durable recovery state for every owned task; and
- the candidate binding and proposed migration plan digest.

Any missing or unreadable protected root, mismatched store identity, divergent journal, failed
artifact integrity check, unquiesced nonrecoverable task, or insufficient bounded storage stops the
upgrade before mutation.

## Backup procedure

1. Request orderly daemon shutdown through the G0 ownership boundary and observe process-tree exit.
2. Prevent supervisor restart while preserving the prior supervisor definition as an artifact.
3. Open durable state read-only and verify journal, snapshot, projection, and blob integrity.
4. Create a candidate-scoped backup in a new sibling staging directory. Copy configuration,
   journal/database files, snapshots, required blobs, migration metadata, and the prior package
   manifest according to their existing ownership contracts. Credential values remain in their
   platform credential store; record only stable credential references.
5. Flush files and parent-directory metadata through the platform durability adapter.
6. Inventory every backup file by normalized relative path, byte length, SHA-256, media type, and
   role. Reopen and verify the completed backup against that inventory.
7. Mark the backup complete only by the artifact store's atomic finalization boundary. A staging
   directory without this marker is incomplete and cannot be selected for restore.

The evidence record includes the backup inventory digest, source journal head/schema/store identity,
destination filesystem identity, bytes retained, flush observations, finalization identity, and
the absence of secret material from exported diagnostics.

## Forward migration procedure

1. Verify candidate package, artifact manifest, SBOM, provenance, and detached signatures before
   executing candidate bytes.
2. Snapshot package-owned files independently of the durable-state backup.
3. Publish candidate package files through temporary siblings and atomic replacement; apply native
   owner-only modes or ACLs and the reviewed supervisor definition.
4. Start the candidate in migration mode through the production G0 composition. Each historical
   migration runs exactly once through C0's transaction and hash-chain checks.
5. After each migration boundary, record prior/new schema versions, migration identity, journal
   head, store identity, transaction result, and recovery marker. A process interruption must yield
   either the complete prior boundary or the complete new boundary.
6. Rebuild disposable projections from the authoritative journal and compare the resulting
   acceptance decisions and compatibility corpus to the retained expected digests.
7. Start normal service, authenticate the local endpoint, execute the documented smoke contract,
   and verify protected configuration, state, logs, and credential references remain outside the
   package mutation set.

The forward result is failed if a migration is skipped, repeated incompatibly, partially committed,
changes store identity, diverges the journal, loses a required blob, changes an acceptance result,
or requires manual database editing.

## Restore procedure

Restore is a controlled replacement of durable state, not a file overlay.

1. Stop the daemon, disable supervisor restart, and prove that no process retains the state root.
2. Select a finalized backup whose manifest and detached signature verify and whose store identity
   matches the intended installation.
3. Preserve the failed state root as a separately content-addressed diagnostic artifact when policy
   and storage bounds allow. Never merge it into the backup.
4. Materialize the backup into a new sibling root; verify every file hash, expected owner/mode or
   ACL, journal chain, schema version, snapshot, and required blob.
5. Atomically exchange the state-root pointer or directory through the platform recovery adapter.
6. Reopen the restored store, rebuild disposable projections, and compare the journal head and
   acceptance decisions to the backup manifest.
7. Re-enable the supervisor, start the package version compatible with the restored schema, and
   require authenticated readiness before declaring restoration complete.

If any verification or atomic replacement step fails, leave the original root selected, retain the
new sibling as failed diagnostic material within bounds, and report a non-success terminal state.

## Package rollback procedure

Package rollback is triggered when candidate installation, migration, startup, or readiness fails.

1. Stop and completely reap the candidate process tree.
2. Preserve bounded candidate diagnostics and the failed-step identity.
3. Determine whether the durable schema crossed a non-reversible boundary. If it did, select the
   verified pre-migration backup and execute the restore procedure before starting prior code.
4. Restore the prior package manifest, binaries, helper, and supervisor definition from their
   independently verified package snapshot.
5. Reapply native owner-only modes or ACLs, register the prior supervisor definition, and start the
   prior package.
6. Require authenticated readiness, exact store identity, expected journal head, replay/projection
   integrity, and preservation of configuration, logs, evidence, and credential references.

A rollback that restores binaries but leaves an incompatible durable schema is failed. A rollback
that requires manual state repair is failed. The operator must not label the candidate ready because
the prior release recovered successfully.

## Required recovery drills

H4 retains signed observations for at least these independent fresh-subject cases:

| Drill | Injected boundary | Required observation |
| --- | --- | --- |
| Backup interruption | During file copy and before finalization | Incomplete staging is rejected; source is unchanged. |
| Package publication interruption | Before and after atomic replacement | Either prior or candidate package is complete; no mixed manifest. |
| Migration interruption | Every C0 migration commit point | Prior or new schema boundary is complete and journal-valid. |
| Candidate startup failure | Before authenticated readiness | Candidate is reaped and rollback begins. |
| Restore interruption | Before and after state-root exchange | One complete verified root remains selected. |
| Prior-package restart failure | During rollback | Rollback remains failed; no success or readiness claim. |
| Projection corruption | After restore, before readiness | Rebuild from journal recovers or reports explicit corruption. |
| Missing/corrupt blob | Backup verification and restored open | Backup/restore is rejected before readiness. |
| Disk/resource exhaustion | Backup, migration, and restore writes | Bounded non-success with source state preserved. |
| Second execution | Reapply completed migration/rollback | Idempotent result or explicit safe rejection; no duplicated transition. |

Each drill records the fresh subject identity, exact candidate and prior version, fault location,
before/after hashes, process cleanup, retained-resource counts, and detached signature envelope.

## H4 admission

H4 admits migration and recovery evidence only when the migration, backup, restore, rollback,
license-notice, and security-review documents each occur exactly once in the candidate-bound
`DocumentationInventory`. The relevant signed payload must match that inventory's canonical digest,
the evidence manifest must retain it, and AC-07, AC-14, AC-16, AC-18, AC-24, and AC-25 must reference
the applicable observations.

Documentation does not make a drill pass. Missing execution, incomplete cleanup, an open blocking
finding, a reused subject, a digest mismatch, or policy unavailability leaves the release not ready.
