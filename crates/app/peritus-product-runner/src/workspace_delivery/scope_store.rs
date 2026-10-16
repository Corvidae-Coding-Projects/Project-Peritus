//! Transactional path index. Original before-images survive imports and later declarations.

use super::{EnrolledPaths, Entry, FileStamp, ScopedBaseline, absent, failure};
use crate::ProductRunnerError;
use rusqlite::{Connection, OptionalExtension as _, Transaction, params};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::BufRead as _,
    path::{Path, PathBuf},
};

type Result<T> = std::result::Result<T, ProductRunnerError>;
fn db_error(error: impl std::fmt::Display) -> ProductRunnerError {
    failure(format!("scope path index: {error}"))
}

impl ScopedBaseline {
    fn database_path(&self) -> PathBuf {
        let mut path = self.journal.as_os_str().to_os_string();
        path.push(".sqlite3");
        PathBuf::from(path)
    }

    pub(super) fn private_evidence(&self, path: &Path) -> bool {
        if self.journal.starts_with(path) {
            return true;
        }
        let database = self.database_path();
        if database.starts_with(path) {
            return true;
        }
        ["-wal", "-shm", "-journal"].iter().any(|suffix| {
            let mut sidecar = database.as_os_str().to_os_string();
            sidecar.push(suffix);
            Path::new(&sidecar).starts_with(path)
        })
    }

    fn open_index(&self) -> Result<Connection> {
        let database = self.database_path();
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let mut path = database.as_os_str().to_os_string();
            path.push(suffix);
            match fs::symlink_metadata(&path) {
                Ok(meta) if !meta.is_file() || meta.file_type().is_symlink() => {
                    return Err(failure("scope index is not a regular private file"));
                }
                Ok(_) => (),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(error) => return Err(db_error(error)),
            }
        }
        // The original journal is immutable after migration; reject a changed legacy prefix.
        let legacy_bytes = match self.journal_size()? {
            Some(bytes) => bytes,
            None => {
                let mut options = fs::OpenOptions::new();
                options.create_new(true).write(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt as _;
                    options.mode(0o600);
                }
                match options.open(&self.journal) {
                    Ok(file) => file.sync_all().map_err(db_error)?,
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
                    Err(error) => return Err(db_error(error)),
                }
                self.journal_size()?.ok_or_else(|| failure("missing scope journal"))?
            }
        };
        let mut options = fs::OpenOptions::new();
        options.create(true).truncate(false).read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        options.open(&database).map_err(db_error)?;
        let mut connection = Connection::open(&database).map_err(db_error)?;
        connection.busy_timeout(std::time::Duration::from_secs(30)).map_err(db_error)?;
        connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
            CREATE TABLE IF NOT EXISTS metadata (singleton INTEGER PRIMARY KEY CHECK(singleton=1), scope TEXT NOT NULL, legacy_bytes INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS baselines (path TEXT PRIMARY KEY, stamp TEXT NOT NULL) WITHOUT ROWID;
            CREATE TABLE IF NOT EXISTS declarations (path TEXT PRIMARY KEY) WITHOUT ROWID;").map_err(db_error)?;
        let transaction = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(db_error)?;
        let scope = serde_json::to_string(self).map_err(db_error)?;
        let existing: Option<(String, i64)> = transaction
            .query_row("SELECT scope,legacy_bytes FROM metadata WHERE singleton=1", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .optional()
            .map_err(db_error)?;
        if let Some((bound, bytes)) = existing {
            if bound != scope || u64::try_from(bytes).ok() != Some(legacy_bytes) {
                return Err(failure("scope index binding or legacy journal changed"));
            }
        } else {
            if self.journal_size()?.is_some() {
                let file = fs::File::open(&self.journal).map_err(db_error)?;
                let mut reader = std::io::BufReader::new(file);
                let mut bytes = Vec::new();
                loop {
                    bytes.clear();
                    if reader.read_until(b'\n', &mut bytes).map_err(db_error)? == 0 {
                        break;
                    }
                    if bytes.last() != Some(&b'\n') {
                        return Err(failure(
                            "in-place evidence has a torn record; effects remain unverified",
                        ));
                    }
                    let entry: Entry = serde_json::from_slice(&bytes).map_err(db_error)?;
                    self.observed_path(&entry.path)?;
                    if entry.scope != *self
                        || !matches!(entry.version, 1 | 2)
                        || (entry.version == 1 && !entry.descendants.is_empty())
                    {
                        return Err(failure("invalid legacy scope journal binding"));
                    }
                    if transaction
                        .execute("INSERT OR IGNORE INTO declarations VALUES (?1)", [&entry.path])
                        .map_err(db_error)?
                        != 1
                    {
                        return Err(failure("duplicate legacy scope declaration"));
                    }
                    for (path, stamp) in entry.descendants {
                        if path == entry.path || !Path::new(&path).starts_with(&entry.path) {
                            return Err(failure("directory evidence escaped its declared subtree"));
                        }
                        self.observed_path(&path)?;
                        insert(&transaction, &path, &stamp)?;
                    }
                    insert(&transaction, &entry.path, &entry.before)?;
                }
            }
            transaction
                .execute(
                    "INSERT INTO metadata VALUES (1,?1,?2)",
                    params![scope, i64::try_from(legacy_bytes).map_err(db_error)?],
                )
                .map_err(db_error)?;
        }
        transaction.commit().map_err(db_error)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&database, fs::Permissions::from_mode(0o600)).map_err(db_error)?;
            if let Some(parent) = database.parent() {
                fs::File::open(parent).and_then(|file| file.sync_all()).map_err(db_error)?;
            }
        }
        Ok(connection)
    }

    pub(super) fn load_index(&self) -> Result<EnrolledPaths> {
        let connection = self.open_index()?;
        // One snapshot binds declarations and stamps even if another process enrolls a path.
        connection.execute_batch("BEGIN DEFERRED").map_err(db_error)?;
        let mut entries = BTreeMap::new();
        let mut query = connection
            .prepare("SELECT path,stamp FROM baselines ORDER BY path")
            .map_err(db_error)?;
        let rows = query
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
            .map_err(db_error)?;
        for row in rows {
            let (path, stamp) = row.map_err(db_error)?;
            self.observed_path(&path)?;
            entries.insert(path, serde_json::from_str(&stamp).map_err(db_error)?);
        }
        let mut declarations = BTreeSet::new();
        let mut query =
            connection.prepare("SELECT path FROM declarations ORDER BY path").map_err(db_error)?;
        for path in query.query_map([], |row| row.get::<_, String>(0)).map_err(db_error)? {
            declarations.insert(path.map_err(db_error)?);
        }
        Ok(EnrolledPaths { entries, declarations })
    }

    pub(super) fn enroll_index(&self, path: &str) -> Result<()> {
        self.checked_path(path)?;
        let mut connection = self.open_index()?;
        let transaction = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(db_error)?;
        if transaction
            .query_row("SELECT 1 FROM declarations WHERE path=?1", [path], |_| Ok(()))
            .optional()
            .map_err(db_error)?
            .is_some()
        {
            return Ok(());
        }
        let original: Option<String> = transaction
            .query_row("SELECT stamp FROM baselines WHERE path=?1", [path], |row| row.get(0))
            .optional()
            .map_err(db_error)?;
        let before: FileStamp = if let Some(original) = original {
            let before: FileStamp = serde_json::from_str(&original).map_err(db_error)?;
            if before.kind == "directory" {
                return Ok(());
            }
            before
        } else {
            let mut covered = false;
            // Parent discovery deliberately omits caches/private paths. A separately
            // authorized exact cache file therefore needs its actual original stamp.
            let inventoried = !crate::developer_tools::ignored(Path::new(path));
            for ancestor in
                Path::new(path).ancestors().skip(1).filter(|path| !path.as_os_str().is_empty())
            {
                if !inventoried {
                    break;
                }
                let Some(ancestor) = ancestor.to_str() else { continue };
                let stamp: Option<String> = transaction
                    .query_row("SELECT stamp FROM baselines WHERE path=?1", [ancestor], |row| {
                        row.get(0)
                    })
                    .optional()
                    .map_err(db_error)?;
                // A formerly absent/file path may now be a directory; its newly created
                // descendants were absent before the effect, too.
                if stamp.is_some() {
                    covered = true;
                    break;
                }
            }
            if covered { absent() } else { self.stamp(path)? }
        };
        if before.kind == "directory" {
            self.visit_subtree(path, true, |child| {
                insert(&transaction, child, &self.stamp(child)?)
            })?;
        }
        insert(&transaction, path, &before)?;
        transaction.execute("INSERT INTO declarations VALUES (?1)", [path]).map_err(db_error)?;
        transaction.commit().map_err(db_error)?;
        Ok(())
    }
}

fn insert(transaction: &Transaction<'_>, path: &str, stamp: &FileStamp) -> Result<()> {
    let stamp = serde_json::to_string(stamp).map_err(db_error)?;
    transaction
        .execute("INSERT OR IGNORE INTO baselines VALUES (?1,?2)", params![path, stamp])
        .map_err(db_error)?;
    Ok(())
}
