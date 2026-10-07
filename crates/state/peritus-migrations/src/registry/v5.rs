//! Persistent delivery for accepted debugger model and publication directives.

pub(super) const SQL: &str = r"ALTER TABLE outbox ADD COLUMN persistent INTEGER NOT NULL DEFAULT 0
    CHECK (persistent IN (0, 1));
UPDATE outbox
   SET persistent = 1,
       state = CASE WHEN state = 4 THEN 1 ELSE state END
 WHERE destination IN (
    'peritus.debugger.model-analysis.v1',
    'peritus.debugger.publish-report.v1'
 );
UPDATE store_meta SET schema_version = 5 WHERE singleton = 1;
PRAGMA user_version = 5;
";
pub(super) const DIGEST: [u8; 32] = [
    0x6b, 0x66, 0x54, 0x8d, 0x32, 0xa8, 0x40, 0x45, 0x10, 0xcd, 0x48, 0x2f, 0x9f, 0x7d, 0x1a, 0x78,
    0x36, 0x14, 0x65, 0xf2, 0xde, 0x69, 0x02, 0x93, 0xea, 0xdd, 0xad, 0xe4, 0x14, 0xfb, 0x77, 0xf5,
];
