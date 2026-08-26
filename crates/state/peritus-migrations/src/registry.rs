//! Immutable ordered migration registry.

mod v1;
mod v5;
mod v6;
mod v7;
mod validation;

use peritus_types::Sha256Digest;
use sha2::{Digest, Sha256};

use crate::{
    BackupPolicy, MigrationDescriptor, MigrationError, MigrationErrorCode, MigrationVersion,
    RecoveryClass, verified::versions_are_contiguous,
};
use validation::{invalid_registry, reject_transaction_control};

const VERSION_TWO_SQL: &str = r"PRAGMA defer_foreign_keys = ON;
CREATE TABLE aggregate_heads_v2 (
    aggregate_kind INTEGER NOT NULL CHECK (aggregate_kind BETWEEN 1 AND 6),
    aggregate_id BLOB NOT NULL CHECK (length(aggregate_id) = 16),
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    event_id BLOB NOT NULL CHECK (length(event_id) = 16),
    event_hash BLOB NOT NULL CHECK (length(event_hash) = 32),
    PRIMARY KEY (aggregate_kind, aggregate_id)
) STRICT, WITHOUT ROWID;
INSERT INTO aggregate_heads_v2(
    aggregate_kind, aggregate_id, sequence, event_id, event_hash
)
SELECT aggregate_kind, aggregate_id, sequence, event_id, event_hash
FROM aggregate_heads ORDER BY aggregate_kind, aggregate_id;
CREATE TEMP TABLE migration_v2_head_count(
    valid INTEGER NOT NULL CHECK (valid = 1)
) STRICT;
INSERT INTO migration_v2_head_count(valid)
SELECT COUNT(*) = (SELECT COUNT(*) FROM aggregate_heads_v2) FROM aggregate_heads;
DROP TABLE aggregate_heads;
ALTER TABLE aggregate_heads_v2 RENAME TO aggregate_heads;
DROP TABLE migration_v2_head_count;
CREATE TABLE events_v2 (
    global_position INTEGER PRIMARY KEY AUTOINCREMENT CHECK (global_position > 0),
    event_id BLOB NOT NULL UNIQUE CHECK (length(event_id) = 16),
    aggregate_kind INTEGER NOT NULL CHECK (aggregate_kind BETWEEN 1 AND 6),
    aggregate_id BLOB NOT NULL CHECK (length(aggregate_id) = 16),
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    previous_event_id BLOB CHECK (previous_event_id IS NULL OR length(previous_event_id) = 16),
    previous_event_hash BLOB NOT NULL CHECK (length(previous_event_hash) = 32),
    event_hash BLOB NOT NULL CHECK (length(event_hash) = 32),
    command_id BLOB NOT NULL CHECK (length(command_id) = 16),
    frame_family INTEGER NOT NULL CHECK (frame_family > 0),
    frame_schema INTEGER NOT NULL CHECK (frame_schema > 0),
    frame_digest BLOB NOT NULL CHECK (length(frame_digest) = 32),
    revision_digest BLOB NOT NULL CHECK (length(revision_digest) = 32),
    causal_ids BLOB NOT NULL CHECK ((length(causal_ids) % 16) = 0),
    frame BLOB NOT NULL,
    UNIQUE (aggregate_kind, aggregate_id, sequence)
) STRICT;
INSERT INTO events_v2(
    global_position, event_id, aggregate_kind, aggregate_id, sequence, previous_event_id,
    previous_event_hash, event_hash, command_id, frame_family, frame_schema, frame_digest,
    revision_digest, causal_ids, frame
)
SELECT global_position, event_id, aggregate_kind, aggregate_id, sequence, previous_event_id,
       previous_event_hash, event_hash, command_id, frame_family, frame_schema, frame_digest,
       revision_digest, causal_ids, frame
FROM events ORDER BY global_position;
CREATE TEMP TABLE migration_v2_event_count(
    valid INTEGER NOT NULL CHECK (valid = 1)
) STRICT;
INSERT INTO migration_v2_event_count(valid)
SELECT COUNT(*) = (SELECT COUNT(*) FROM events_v2) FROM events;
DROP TABLE events;
ALTER TABLE events_v2 RENAME TO events;
CREATE INDEX events_command ON events(command_id, global_position);
DROP TABLE migration_v2_event_count;
UPDATE store_meta SET schema_version = 2 WHERE singleton = 1 AND schema_version = 1;
CREATE TEMP TABLE migration_v2_meta_check(
    valid INTEGER NOT NULL CHECK (valid = 1)
) STRICT;
INSERT INTO migration_v2_meta_check(valid)
SELECT COUNT(*) = 1 FROM store_meta WHERE singleton = 1 AND schema_version = 2;
DROP TABLE migration_v2_meta_check;
PRAGMA user_version = 2;
";
// Updated whenever the reviewed exact VERSION_TWO_SQL source changes.
const VERSION_TWO_DIGEST: [u8; 32] = [
    0xb5, 0x35, 0x45, 0xa8, 0xbf, 0x5c, 0x04, 0x13, 0x4f, 0xc6, 0xc9, 0x0b, 0xfa, 0x34, 0xb9, 0xb6,
    0x81, 0x16, 0x9d, 0x28, 0x73, 0xca, 0xac, 0x67, 0x60, 0xc9, 0x6d, 0x0e, 0x40, 0x63, 0x91, 0x71,
];
const VERSION_THREE_SQL: &str = r"PRAGMA defer_foreign_keys = ON;
CREATE TABLE aggregate_heads_v3 (
    aggregate_kind INTEGER NOT NULL CHECK (aggregate_kind BETWEEN 1 AND 8),
    aggregate_id BLOB NOT NULL CHECK (length(aggregate_id) = 16),
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    event_id BLOB NOT NULL CHECK (length(event_id) = 16),
    event_hash BLOB NOT NULL CHECK (length(event_hash) = 32),
    PRIMARY KEY (aggregate_kind, aggregate_id)
) STRICT, WITHOUT ROWID;
INSERT INTO aggregate_heads_v3(
    aggregate_kind, aggregate_id, sequence, event_id, event_hash
)
SELECT aggregate_kind, aggregate_id, sequence, event_id, event_hash
FROM aggregate_heads ORDER BY aggregate_kind, aggregate_id;
CREATE TEMP TABLE migration_v3_head_count(
    valid INTEGER NOT NULL CHECK (valid = 1)
) STRICT;
INSERT INTO migration_v3_head_count(valid)
SELECT COUNT(*) = (SELECT COUNT(*) FROM aggregate_heads_v3) FROM aggregate_heads;
DROP TABLE aggregate_heads;
ALTER TABLE aggregate_heads_v3 RENAME TO aggregate_heads;
DROP TABLE migration_v3_head_count;
CREATE TABLE events_v3 (
    global_position INTEGER PRIMARY KEY AUTOINCREMENT CHECK (global_position > 0),
    event_id BLOB NOT NULL UNIQUE CHECK (length(event_id) = 16),
    aggregate_kind INTEGER NOT NULL CHECK (aggregate_kind BETWEEN 1 AND 8),
    aggregate_id BLOB NOT NULL CHECK (length(aggregate_id) = 16),
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    previous_event_id BLOB CHECK (previous_event_id IS NULL OR length(previous_event_id) = 16),
    previous_event_hash BLOB NOT NULL CHECK (length(previous_event_hash) = 32),
    event_hash BLOB NOT NULL CHECK (length(event_hash) = 32),
    command_id BLOB NOT NULL CHECK (length(command_id) = 16),
    frame_family INTEGER NOT NULL CHECK (frame_family > 0),
    frame_schema INTEGER NOT NULL CHECK (frame_schema > 0),
    frame_digest BLOB NOT NULL CHECK (length(frame_digest) = 32),
    revision_digest BLOB NOT NULL CHECK (length(revision_digest) = 32),
    causal_ids BLOB NOT NULL CHECK ((length(causal_ids) % 16) = 0),
    frame BLOB NOT NULL,
    UNIQUE (aggregate_kind, aggregate_id, sequence)
) STRICT;
INSERT INTO events_v3(
    global_position, event_id, aggregate_kind, aggregate_id, sequence, previous_event_id,
    previous_event_hash, event_hash, command_id, frame_family, frame_schema, frame_digest,
    revision_digest, causal_ids, frame
)
SELECT global_position, event_id, aggregate_kind, aggregate_id, sequence, previous_event_id,
       previous_event_hash, event_hash, command_id, frame_family, frame_schema, frame_digest,
       revision_digest, causal_ids, frame
FROM events ORDER BY global_position;
CREATE TEMP TABLE migration_v3_event_count(
    valid INTEGER NOT NULL CHECK (valid = 1)
) STRICT;
INSERT INTO migration_v3_event_count(valid)
SELECT COUNT(*) = (SELECT COUNT(*) FROM events_v3) FROM events;
DROP TABLE events;
ALTER TABLE events_v3 RENAME TO events;
CREATE INDEX events_command ON events(command_id, global_position);
DROP TABLE migration_v3_event_count;
UPDATE store_meta SET schema_version = 3 WHERE singleton = 1 AND schema_version = 2;
CREATE TEMP TABLE migration_v3_meta_check(
    valid INTEGER NOT NULL CHECK (valid = 1)
) STRICT;
INSERT INTO migration_v3_meta_check(valid)
SELECT COUNT(*) = 1 FROM store_meta WHERE singleton = 1 AND schema_version = 3;
DROP TABLE migration_v3_meta_check;
PRAGMA user_version = 3;
";
// Updated whenever the reviewed exact VERSION_THREE_SQL source changes.
const VERSION_THREE_DIGEST: [u8; 32] = [
    0xfc, 0x15, 0xf0, 0xfc, 0x92, 0x6d, 0xcb, 0x83, 0x3a, 0xfe, 0x62, 0xc5, 0x2b, 0xd9, 0x7e, 0x29,
    0xb6, 0xe6, 0x40, 0x0e, 0x6d, 0xe6, 0xe6, 0xfc, 0xbc, 0x56, 0xf9, 0xd9, 0x13, 0x6e, 0xe9, 0xa5,
];
const VERSION_FOUR_SQL: &str = r"PRAGMA defer_foreign_keys = ON;
CREATE TABLE aggregate_heads_v4 (
    aggregate_kind INTEGER NOT NULL CHECK (aggregate_kind BETWEEN 1 AND 9),
    aggregate_id BLOB NOT NULL CHECK (length(aggregate_id) = 16),
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    event_id BLOB NOT NULL CHECK (length(event_id) = 16),
    event_hash BLOB NOT NULL CHECK (length(event_hash) = 32),
    PRIMARY KEY (aggregate_kind, aggregate_id)
) STRICT, WITHOUT ROWID;
INSERT INTO aggregate_heads_v4(
    aggregate_kind, aggregate_id, sequence, event_id, event_hash
)
SELECT aggregate_kind, aggregate_id, sequence, event_id, event_hash
FROM aggregate_heads ORDER BY aggregate_kind, aggregate_id;
CREATE TEMP TABLE migration_v4_head_count(
    valid INTEGER NOT NULL CHECK (valid = 1)
) STRICT;
INSERT INTO migration_v4_head_count(valid)
SELECT COUNT(*) = (SELECT COUNT(*) FROM aggregate_heads_v4) FROM aggregate_heads;
DROP TABLE aggregate_heads;
ALTER TABLE aggregate_heads_v4 RENAME TO aggregate_heads;
DROP TABLE migration_v4_head_count;
CREATE TABLE events_v4 (
    global_position INTEGER PRIMARY KEY AUTOINCREMENT CHECK (global_position > 0),
    event_id BLOB NOT NULL UNIQUE CHECK (length(event_id) = 16),
    aggregate_kind INTEGER NOT NULL CHECK (aggregate_kind BETWEEN 1 AND 9),
    aggregate_id BLOB NOT NULL CHECK (length(aggregate_id) = 16),
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    previous_event_id BLOB CHECK (previous_event_id IS NULL OR length(previous_event_id) = 16),
    previous_event_hash BLOB NOT NULL CHECK (length(previous_event_hash) = 32),
    event_hash BLOB NOT NULL CHECK (length(event_hash) = 32),
    command_id BLOB NOT NULL CHECK (length(command_id) = 16),
    frame_family INTEGER NOT NULL CHECK (frame_family > 0),
    frame_schema INTEGER NOT NULL CHECK (frame_schema > 0),
    frame_digest BLOB NOT NULL CHECK (length(frame_digest) = 32),
    revision_digest BLOB NOT NULL CHECK (length(revision_digest) = 32),
    causal_ids BLOB NOT NULL CHECK ((length(causal_ids) % 16) = 0),
    frame BLOB NOT NULL,
    UNIQUE (aggregate_kind, aggregate_id, sequence)
) STRICT;
INSERT INTO events_v4(
    global_position, event_id, aggregate_kind, aggregate_id, sequence, previous_event_id,
    previous_event_hash, event_hash, command_id, frame_family, frame_schema, frame_digest,
    revision_digest, causal_ids, frame
)
SELECT global_position, event_id, aggregate_kind, aggregate_id, sequence, previous_event_id,
       previous_event_hash, event_hash, command_id, frame_family, frame_schema, frame_digest,
       revision_digest, causal_ids, frame
FROM events ORDER BY global_position;
CREATE TEMP TABLE migration_v4_event_count(
    valid INTEGER NOT NULL CHECK (valid = 1)
) STRICT;
INSERT INTO migration_v4_event_count(valid)
SELECT COUNT(*) = (SELECT COUNT(*) FROM events_v4) FROM events;
DROP TABLE events;
ALTER TABLE events_v4 RENAME TO events;
CREATE INDEX events_command ON events(command_id, global_position);
DROP TABLE migration_v4_event_count;
UPDATE store_meta SET schema_version = 4 WHERE singleton = 1 AND schema_version = 3;
CREATE TEMP TABLE migration_v4_meta_check(
    valid INTEGER NOT NULL CHECK (valid = 1)
) STRICT;
INSERT INTO migration_v4_meta_check(valid)
SELECT COUNT(*) = 1 FROM store_meta WHERE singleton = 1 AND schema_version = 4;
DROP TABLE migration_v4_meta_check;
PRAGMA user_version = 4;
";
// Updated whenever the reviewed exact VERSION_FOUR_SQL source changes.
const VERSION_FOUR_DIGEST: [u8; 32] = [
    0x5d, 0x9e, 0x44, 0x2b, 0x23, 0xd0, 0x47, 0xbb, 0xf4, 0x2f, 0xe0, 0xcd, 0xc6, 0xfc, 0xfe, 0x1c,
    0x2c, 0x66, 0x91, 0x01, 0xd4, 0x17, 0x6b, 0x4c, 0xaa, 0x9d, 0x50, 0xe0, 0xda, 0x12, 0x3c, 0xb6,
];
const CURRENT_DESCRIPTORS: [MigrationDescriptor; 7] = [
    MigrationDescriptor::new(
        MigrationVersion::FIRST,
        "0.0.0",
        v1::SQL,
        Sha256Digest::new(v1::DIGEST),
        BackupPolicy::Required,
        64 * 1024,
    ),
    MigrationDescriptor::new(
        MigrationVersion::SECOND,
        "0.0.0",
        VERSION_TWO_SQL,
        Sha256Digest::new(VERSION_TWO_DIGEST),
        BackupPolicy::Required,
        32 * 1024 * 1024,
    ),
    MigrationDescriptor::new(
        MigrationVersion::THIRD,
        "0.0.0",
        VERSION_THREE_SQL,
        Sha256Digest::new(VERSION_THREE_DIGEST),
        BackupPolicy::Required,
        32 * 1024 * 1024,
    ),
    MigrationDescriptor::new(
        MigrationVersion::FOURTH,
        "0.0.0",
        VERSION_FOUR_SQL,
        Sha256Digest::new(VERSION_FOUR_DIGEST),
        BackupPolicy::Required,
        32 * 1024 * 1024,
    ),
    MigrationDescriptor::new(
        MigrationVersion::FIFTH,
        "0.0.0",
        v5::SQL,
        Sha256Digest::new(v5::DIGEST),
        BackupPolicy::Required,
        32 * 1024 * 1024,
    ),
    MigrationDescriptor::new(
        MigrationVersion::SIXTH,
        "0.0.0",
        v6::SQL,
        Sha256Digest::new(v6::DIGEST),
        BackupPolicy::Required,
        32 * 1024 * 1024,
    ),
    MigrationDescriptor::new(
        MigrationVersion::SEVENTH,
        "0.0.0",
        v7::SQL,
        Sha256Digest::new(v7::DIGEST),
        BackupPolicy::Required,
        32 * 1024 * 1024,
    ),
];

/// Immutable ordered migration registry.
#[derive(Clone, Copy, Debug)]
pub struct MigrationRegistry {
    descriptors: &'static [MigrationDescriptor],
}

impl MigrationRegistry {
    /// Returns the compiled production registry.
    #[must_use]
    pub const fn current() -> Self {
        Self { descriptors: &CURRENT_DESCRIPTORS }
    }

    /// Creates a registry from statically compiled descriptors.
    ///
    /// This constructor supports compatibility fixtures and future composed registries. Every use
    /// still recomputes exact source digests and validates ordering.
    #[must_use]
    pub const fn from_static(descriptors: &'static [MigrationDescriptor]) -> Self {
        Self { descriptors }
    }

    /// Returns descriptors in required application order.
    #[must_use]
    pub const fn descriptors(self) -> &'static [MigrationDescriptor] {
        self.descriptors
    }

    /// Validates nonempty contiguous ordering, exact digests, release text, and safe transaction
    /// ownership of migration SQL.
    ///
    /// # Errors
    ///
    /// Returns a stable registry or digest-drift error.
    pub fn validate(self) -> Result<(), MigrationError> {
        if self.descriptors.is_empty() {
            return Err(invalid_registry("migration registry must not be empty"));
        }
        let mut previous = 0_u64;
        for descriptor in self.descriptors {
            if !versions_are_contiguous(previous, descriptor.version().get()) {
                return Err(invalid_registry("migration versions are not contiguous from one"));
            }
            if descriptor.release().is_empty()
                || descriptor.release().len() > 128
                || !descriptor.release().bytes().all(|byte| byte.is_ascii_graphic())
            {
                return Err(invalid_registry("migration release text is invalid"));
            }
            if descriptor.sql().is_empty() || descriptor.sql().len() > 4 * 1024 * 1024 {
                return Err(invalid_registry("migration SQL source is empty or too large"));
            }
            reject_transaction_control(descriptor.sql())?;
            let actual = Sha256Digest::new(Sha256::digest(descriptor.sql().as_bytes()).into());
            if actual != descriptor.source_digest() {
                return Err(MigrationError::message(
                    MigrationErrorCode::DigestDrift,
                    RecoveryClass::Terminal,
                    "validate migration registry",
                    "migration source digest differs from the reviewed descriptor",
                ));
            }
            previous = descriptor.version().get();
        }
        Ok(())
    }

    /// Returns the latest compiled version after validation.
    ///
    /// # Errors
    ///
    /// Returns the same failures as [`Self::validate`].
    pub fn latest(self) -> Result<MigrationVersion, MigrationError> {
        self.validate()?;
        self.descriptors
            .last()
            .map(|descriptor| descriptor.version())
            .ok_or_else(|| invalid_registry("migration registry must not be empty"))
    }

    /// Computes a digest binding the complete ordered registry.
    ///
    /// # Errors
    ///
    /// Returns the same failures as [`Self::validate`].
    pub fn digest(self) -> Result<Sha256Digest, MigrationError> {
        self.validate()?;
        let mut hasher = Sha256::new();
        hasher.update(b"peritus/migration-registry/v1\0");
        for descriptor in self.descriptors {
            hasher.update(descriptor.version().get().to_be_bytes());
            hasher.update(descriptor.source_digest().as_bytes());
            hasher.update([match descriptor.backup_policy() {
                BackupPolicy::NotRequired => 0,
                BackupPolicy::Required => 1,
            }]);
            hasher.update(descriptor.scratch_bytes().to_be_bytes());
            hasher.update((descriptor.release().len() as u64).to_be_bytes());
            hasher.update(descriptor.release().as_bytes());
        }
        Ok(Sha256Digest::new(hasher.finalize().into()))
    }
}
