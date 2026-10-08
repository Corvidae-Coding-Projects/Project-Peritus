//! Persistent delivery for accepted E3 schedule, execution, and publication directives.

pub(super) const SQL: &str = r"UPDATE outbox
   SET persistent = 1,
       state = CASE WHEN state = 4 THEN 1 ELSE state END
 WHERE destination IN (
    'peritus.eval.schedule-rollout.v1',
    'peritus.eval.execute-rollout.v1',
    'peritus.eval.publish-report.v1'
 );
UPDATE store_meta SET schema_version = 6 WHERE singleton = 1;
PRAGMA user_version = 6;
";
pub(super) const DIGEST: [u8; 32] = [
    0xe7, 0x77, 0x46, 0xbc, 0x9f, 0x6a, 0x7c, 0x33, 0xc4, 0xfb, 0x63, 0x30, 0x11, 0x52, 0x6e, 0xea,
    0x57, 0x80, 0x99, 0xab, 0x27, 0xcb, 0xce, 0xb9, 0x43, 0x51, 0x79, 0x14, 0x58, 0x31, 0x97, 0x1a,
];
