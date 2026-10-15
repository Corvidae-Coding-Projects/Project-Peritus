//! Preserve all state bytes while separating large values from lookup keys.

pub(super) const SQL: &str = r"CREATE TABLE state_records_v4 (
    namespace INTEGER NOT NULL CHECK (namespace > 0),
    record_key BLOB NOT NULL CHECK (length(record_key) BETWEEN 1 AND 1024),
    revision INTEGER NOT NULL CHECK (revision > 0),
    value_digest BLOB NOT NULL CHECK (length(value_digest) = 32),
    value BLOB NOT NULL,
    producing_position INTEGER NOT NULL REFERENCES events(global_position),
    PRIMARY KEY (namespace, record_key)
) STRICT;
INSERT INTO state_records_v4 SELECT * FROM state_records;
DROP TABLE state_records;
ALTER TABLE state_records_v4 RENAME TO state_records;
UPDATE store_meta SET schema_version = 4 WHERE singleton = 1;
PRAGMA user_version = 4;
";
pub(super) const DIGEST: [u8; 32] = [
    0x77, 0x75, 0xde, 0x1e, 0x3c, 0x98, 0xb0, 0x8f, 0xc5, 0x6f, 0x68, 0xe0, 0x3a, 0xd8, 0xd8, 0xf1,
    0x5c, 0x6b, 0x90, 0x72, 0xff, 0x4e, 0x91, 0x88, 0xb2, 0xac, 0x66, 0xee, 0x37, 0x3b, 0x70, 0xdc,
];
