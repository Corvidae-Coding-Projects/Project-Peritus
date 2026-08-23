-- Historical unversioned fixture layered onto the frozen journal schema by the test harness.
PRAGMA user_version = 0;
CREATE TABLE migration_fixture_payload (
    fixture_key TEXT PRIMARY KEY NOT NULL,
    fixture_value BLOB NOT NULL
) STRICT;
INSERT INTO migration_fixture_payload(fixture_key, fixture_value)
VALUES ('preserved', X'70657269747573');
