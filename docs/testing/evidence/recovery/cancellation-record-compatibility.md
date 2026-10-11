# Cancellation record migration and rollback

The durable `user_cancelled` boolean was added so a cancellation acknowledged immediately before
process death cannot be restored as automatically resumable work. New readers default an absent
field to `false`, so records written by older versions retain their previous recovery behavior.
`cancelled_recovery_record_does_not_become_automatically_resumable` verifies the new-reader path.

An older daemon ignores the new field. A nonterminal or `RecoveryRequired` record with
`user_cancelled = true` could therefore become resumable after a direct binary downgrade. The raw
phase may also carry the existing interaction offset: zero for build records, 100 for interactive
records, and 200 for workbench records. A direct downgrade is unsafe while any affected record
exists.

Before installing an older daemon:

1. Stop the daemon and confirm that no writer owns the product-run state directory.
2. Copy the complete state directory to a rollback backup on the same durable filesystem.
3. Parse every product-run JSON record. For each record with `user_cancelled = true`, derive its
   offset from the record shape: 200 when `interaction.workbench` exists, 100 when `interaction`
   exists without `workbench`, and zero otherwise. Subtract that offset and require a known stable
   phase tag. Preserve `Complete`, `Failed`, `Cancelled`, and `WaitingForUser` with their existing
   status. For any nonterminal phase or `RecoveryRequired`, replace the base tag with 9
   (`Cancelled`), reapply the same offset, and replace `status` with `Run cancelled`. Then remove
   `user_cancelled`.
4. Write each changed record through a new file, sync the file, rename it over the old record, and
   sync the containing directory. Abort the downgrade on any parse, write, sync, or rename error.
5. Re-read the directory and require that no record contains `user_cancelled = true`, every raw
   phase has the valid offset for its record shape, and every transformed nonterminal or recovery
   record decodes to `Cancelled` after removing that offset. Only then may the older daemon start.

`cancelled_record_can_be_transformed_for_safe_legacy_downgrade` exercises recovery, already
cancelled, completed, and offset interactive records. It verifies that the transform materializes
cancel intent only where the current reader would do so and preserves existing terminal outcomes.
Restoration of the pre-transform backup requires reinstalling the new reader first; the backup
must never be opened by an old daemon because it contains cancellation intent the old reader cannot
enforce.

This migration changes only records carrying acknowledged cancellation intent. Records without
the field or with `user_cancelled = false` remain byte-for-byte outside the transform.
