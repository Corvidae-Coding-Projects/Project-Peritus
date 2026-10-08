//! Bounded progress observations and keyset pages over completed directory indexes.

use super::{
    Entry, ListingStore, MaterializationPhase, MaterializationRequest, MaterializationState,
    OWNER_FLAG, PAGE_SIZE, SCHEMA_VERSION, validate_digest, validate_hex_identity, validate_state,
};
use crate::{
    error::{Error, Result, problem, uncertain},
    git::effects,
};
use peritus_process::ProbeObservation;
use rusqlite::{Connection, OpenFlags, params};
use std::{
    fs::File,
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    sync::{Arc, Mutex},
    thread,
};

struct LaunchFallback {
    activation: Option<File>,
    child: Arc<Mutex<Option<Child>>>,
    reap: bool,
}

impl LaunchFallback {
    fn new(activation: File, child: Arc<Mutex<Option<Child>>>) -> Self {
        Self { activation: Some(activation), child, reap: true }
    }

    fn release_activation(&mut self) -> Result<()> {
        let Some(activation) = self.activation.take() else {
            return Ok(());
        };
        let result = fs4::FileExt::unlock(&activation).map_err(problem);
        drop(activation);
        result
    }

    fn wait(&mut self) -> Result<ExitStatus> {
        self.reap = false;
        wait_child(&self.child)
    }

    const fn reaper_started(&mut self) {
        self.reap = false;
    }
}

impl Drop for LaunchFallback {
    fn drop(&mut self) {
        if let Err(error) = self.release_activation() {
            eprintln!("peritus web: directory-index activation release failed: {error}");
        }
        if self.reap
            && let Err(error) = wait_child(&self.child)
        {
            eprintln!("peritus web: directory-index child reaping failed: {error}");
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "every observation must retain its exact directory and cursor bindings"
)]
pub(super) fn list(
    state_file: &Path,
    workspace: &str,
    root: &Path,
    relative: &str,
    expected_directory: &str,
    offset: u64,
    filter: &str,
    cursor: &str,
    expected_snapshot: &str,
) -> Result<serde_json::Value> {
    let store = ListingStore::open(state_file, workspace, root)?;
    if cursor.is_empty() {
        if offset != 0 || !expected_snapshot.is_empty() {
            return Err(problem(
                "A directory continuation requires its durable cursor",
            ));
        }
        let (request, path) = store.create_request(relative, expected_directory, filter)?;
        let state = launch(&store, &request, &path)?;
        return progress(&request, &state);
    }
    if let Some(materialization) = decode_index_cursor(cursor)? {
        if offset != 0 || !expected_snapshot.is_empty() {
            return Err(problem(
                "A directory indexing cursor cannot be used as a completed page cursor",
            ));
        }
        let request = store.request(materialization)?;
        validate_query(&request, relative, expected_directory, filter)?;
        let state = store.state(materialization)?;
        validate_state(&request, &state)?;
        return observe_materialization(&store, &request, &state);
    }
    let (materialization, digest, after) = decode_page_cursor(cursor)?;
    if after != offset {
        return Err(problem(
            "The directory page cursor does not match the requested entry offset",
        ));
    }
    let request = store.request(materialization)?;
    validate_query(&request, relative, expected_directory, filter)?;
    let state = store.state(materialization)?;
    validate_state(&request, &state)?;
    if state.phase != MaterializationPhase::Completed
        || state.digest.as_deref() != Some(digest)
    {
        return Err(problem(
            "The directory page cursor does not identify a completed snapshot",
        ));
    }
    let snapshot = snapshot_cursor(&request, &state)?;
    if expected_snapshot != snapshot {
        return Err(problem(
            "The directory changed; refresh it before loading another page",
        ));
    }
    read_page(&store, &request, &state, after)
}

fn launch(
    store: &ListingStore,
    request: &MaterializationRequest,
    path: &Path,
) -> Result<MaterializationState> {
    let directory = store.materialization_directory(&request.materialization)?;
    let activation = store.activation(&request.materialization)?;
    fs4::FileExt::lock(&activation).map_err(problem)?;
    let mut state = store.state(&request.materialization)?;
    validate_state(request, &state)?;
    if state.phase != MaterializationPhase::Pending || state.owner.is_some() {
        fs4::FileExt::unlock(&activation).map_err(problem)?;
        return Err(uncertain(
            "The new directory materialization already has a launch identity",
        ));
    }
    let executable = std::env::current_exe()?;
    let mut command = Command::new(executable);
    command
        .arg(OWNER_FLAG)
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    configure_owner_command(&mut command);
    let child = command.spawn()?;
    let pid = child.id();
    let child = Arc::new(Mutex::new(Some(child)));
    let mut fallback = LaunchFallback::new(activation, Arc::clone(&child));
    let binding = match effects::detached_launcher_binding(pid) {
        Ok(binding) => binding,
        Err(error) => return Err(failed_launch(&mut fallback, store, request, None, error)),
    };
    state.owner = Some(binding.clone());
    if let Err(error) = super::save_state(&directory, &state) {
        return Err(failed_launch(
            &mut fallback,
            store,
            request,
            Some(&binding),
            error,
        ));
    }
    let reaper_child = Arc::clone(&child);
    let reaper_store = store.clone();
    let reaper_request = request.clone();
    let reaper_binding = binding.clone();
    let reaper = thread::Builder::new()
        .name("peritus-directory-index-reaper".into())
        .spawn(move || {
            let outcome = wait_child(&reaper_child);
            let detail = match &outcome {
                Ok(status) => format!("The directory-index owner stopped ({status})"),
                Err(error) => format!(
                    "The directory-index owner child could not be reaped: {}",
                    error.0,
                ),
            };
            let publication = publish_owner_exit(
                &reaper_store,
                &reaper_request,
                &reaper_binding,
                &detail,
            );
            if let Err(error) = outcome {
                eprintln!("peritus web: {detail}: {error}");
            }
            if let Err(error) = publication {
                eprintln!("peritus web: directory-index owner exit was not published: {error}");
            }
        });
    match reaper {
        Ok(reaper) => {
            fallback.reaper_started();
            drop(reaper);
            fallback.release_activation().map_err(|error| {
                uncertain(format!(
                    "The directory-index owner was published but its activation gate could not be released: {}",
                    error.0,
                ))
            })?;
            Ok(state)
        }
        Err(error) => Err(failed_launch(
            &mut fallback,
            store,
            request,
            Some(&binding),
            uncertain(format!("The directory-index owner reaper could not be started: {error}")),
        )),
    }
}

fn wait_child(child: &Arc<Mutex<Option<Child>>>) -> Result<ExitStatus> {
    let mut child = child
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take()
        .ok_or_else(|| uncertain("The directory-index child handle was lost"))?;
    loop {
        match child.wait() {
            Ok(status) => return Ok(status),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => {
                return Err(uncertain(format!(
                    "The directory-index child could not be reaped: {error}",
                )));
            }
        }
    }
}

fn failed_launch(
    fallback: &mut LaunchFallback,
    store: &ListingStore,
    request: &MaterializationRequest,
    binding: Option<&effects::OwnerBinding>,
    primary: Error,
) -> Error {
    let mut details = vec![primary.0];
    if let Err(error) = fallback.release_activation() {
        details.push(format!("activation release failed: {}", error.0));
    }
    let outcome = fallback.wait();
    let exit_detail = match &outcome {
        Ok(status) => format!("The directory-index owner stopped ({status})"),
        Err(error) => {
            details.push(error.0.clone());
            format!("The directory-index owner child could not be reaped: {}", error.0)
        }
    };
    if let Some(binding) = binding
        && let Err(error) = publish_owner_exit(store, request, binding, &exit_detail)
    {
        details.push(format!("owner exit publication failed: {}", error.0));
    }
    uncertain(details.join("; "))
}

fn publish_owner_exit(
    store: &ListingStore,
    request: &MaterializationRequest,
    binding: &effects::OwnerBinding,
    detail: &str,
) -> Result<()> {
    let owner = store.owner(&request.materialization)?;
    fs4::FileExt::try_lock(&owner).map_err(|error| {
        uncertain(format!(
            "The stopped directory-index owner retained its ownership lock: {error}",
        ))
    })?;
    let mut state = store.state(&request.materialization)?;
    validate_state(request, &state)?;
    if let Some(retained) = &state.owner
        && retained != binding
    {
        fs4::FileExt::unlock(&owner).map_err(problem)?;
        return Err(uncertain(
            "The stopped directory-index owner differs from its durable launch identity",
        ));
    }
    if !matches!(
        state.phase,
        MaterializationPhase::Completed | MaterializationPhase::Failed
    ) {
        state.owner = Some(binding.clone());
        state.phase = MaterializationPhase::Failed;
        state.error = Some(detail.to_owned());
        super::save_state(
            &store.materialization_directory(&request.materialization)?,
            &state,
        )?;
    }
    fs4::FileExt::unlock(&owner).map_err(problem)
}

#[cfg(unix)]
fn configure_owner_command(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(windows)]
fn configure_owner_command(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(
        CREATE_BREAKAWAY_FROM_JOB | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW,
    );
}

#[cfg(not(any(unix, windows)))]
fn configure_owner_command(command: &mut Command) {
    let _ = command;
}

fn observe_materialization(
    store: &ListingStore,
    request: &MaterializationRequest,
    state: &MaterializationState,
) -> Result<serde_json::Value> {
    match state.phase {
        MaterializationPhase::Pending => match observe_owner(state)? {
            ProbeObservation::ExactLive => progress(request, state),
            ProbeObservation::ExactAbsent => stopped_or_terminal(store, request),
            ProbeObservation::Mismatched | ProbeObservation::Unverifiable => Err(uncertain(
                "The exact directory-index owner can no longer be verified",
            )),
        },
        MaterializationPhase::Indexing | MaterializationPhase::Ordering => {
            match observe_owner(state)? {
                ProbeObservation::ExactAbsent => return stopped_or_terminal(store, request),
                ProbeObservation::Mismatched | ProbeObservation::Unverifiable => {
                    return Err(uncertain(
                        "The exact directory-index owner can no longer be verified",
                    ));
                }
                ProbeObservation::ExactLive => {}
            }
            let owner = store.owner(&request.materialization)?;
            match fs4::FileExt::try_lock(&owner) {
                Err(fs4::TryLockError::WouldBlock) => progress(request, state),
                Err(fs4::TryLockError::Error(error)) => Err(error.into()),
                Ok(()) => {
                    fs4::FileExt::unlock(&owner).map_err(problem)?;
                    Err(uncertain(
                        "The directory-index owner stopped before publishing a terminal state",
                    ))
                }
            }
        }
        MaterializationPhase::Completed => read_page(store, request, state, 0),
        MaterializationPhase::Failed => Err(problem(
            state.error.as_deref().unwrap_or("Directory indexing failed"),
        )),
    }
}

fn observe_owner(state: &MaterializationState) -> Result<ProbeObservation> {
    let owner = state
        .owner
        .as_ref()
        .ok_or_else(|| uncertain("The directory materialization has no durable owner identity"))?;
    effects::observe_detached_owner(owner)
}

fn stopped_or_terminal(
    store: &ListingStore,
    request: &MaterializationRequest,
) -> Result<serde_json::Value> {
    let state = store.state(&request.materialization)?;
    validate_state(request, &state)?;
    match state.phase {
        MaterializationPhase::Completed => read_page(store, request, &state, 0),
        MaterializationPhase::Failed => Err(problem(
            state.error.as_deref().unwrap_or("Directory indexing failed"),
        )),
        MaterializationPhase::Pending
        | MaterializationPhase::Indexing
        | MaterializationPhase::Ordering => Err(uncertain(
            "The directory-index owner stopped before publishing a terminal state",
        )),
    }
}

fn progress(
    request: &MaterializationRequest,
    state: &MaterializationState,
) -> Result<serde_json::Value> {
    Ok(serde_json::json!({
        "ready": false,
        "phase": state.phase.as_str(),
        "cursor": index_cursor(&request.materialization),
        "directory": request.directory_identity,
        "filter": request.filter,
        "offset": 0,
        "snapshot": "",
        "entries": Vec::<Entry>::new(),
        "indexed": state.scanned,
        "total": serde_json::Value::Null,
        "next": serde_json::Value::Null,
    }))
}

fn read_page(
    store: &ListingStore,
    request: &MaterializationRequest,
    state: &MaterializationState,
    after: u64,
) -> Result<serde_json::Value> {
    let digest = state
        .digest
        .as_deref()
        .ok_or_else(|| uncertain("The completed directory index has no digest"))?;
    let directory = store.materialization_directory(&request.materialization)?;
    let connection = Connection::open_with_flags(
        directory.join("entries.sqlite3"),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(problem)?;
    connection.pragma_update(None, "query_only", true).map_err(problem)?;
    validate_metadata(&connection, request, state, digest)?;
    if after > state.matched {
        return Err(problem("The directory cursor exceeds its snapshot"));
    }
    let boundary = if after == 0 {
        None
    } else {
        Some(
            connection
                .query_row(
                    "SELECT directory_rank,folded_name,name,path
                     FROM boundaries WHERE position=?1",
                    [i64::try_from(after).map_err(problem)?],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                        ))
                    },
                )
                .map_err(|_| problem("The directory page cursor has no exact keyset boundary"))?,
        )
    };
    let limit = i64::try_from(PAGE_SIZE.checked_add(1).ok_or_else(|| {
        problem("The directory page size exceeds its SQLite representation")
    })?)
    .map_err(problem)?;
    let mut statement = if boundary.is_some() {
        connection
            .prepare(
                "SELECT name,path,directory,directory_identity,symlink,bytes
                 FROM entries
                 WHERE (directory_rank,folded_name,name,path) > (?1,?2,?3,?4)
                 ORDER BY directory_rank,folded_name,name,path
                 LIMIT ?5",
            )
            .map_err(problem)?
    } else {
        connection
            .prepare(
                "SELECT name,path,directory,directory_identity,symlink,bytes
                 FROM entries
                 ORDER BY directory_rank,folded_name,name,path
                 LIMIT ?1",
            )
            .map_err(problem)?
    };
    let mut rows = match &boundary {
        Some((rank, folded, name, path)) => statement
            .query(params![rank, folded, name, path, limit])
            .map_err(problem)?,
        None => statement.query([limit]).map_err(problem)?,
    };
    let mut entries = Vec::with_capacity(usize::try_from(PAGE_SIZE).map_err(problem)? + 1);
    while let Some(row) = rows.next().map_err(problem)? {
        entries.push(Entry {
            name: row.get(0).map_err(problem)?,
            path: row.get(1).map_err(problem)?,
            directory: row.get(2).map_err(problem)?,
            directory_identity: row.get(3).map_err(problem)?,
            symlink: row.get(4).map_err(problem)?,
            bytes: row.get::<_, String>(5).map_err(problem)?.parse().map_err(problem)?,
        });
    }
    let has_more = u64::try_from(entries.len()).map_err(problem)? > PAGE_SIZE;
    if has_more {
        entries.pop();
    }
    let returned = u64::try_from(entries.len()).map_err(problem)?;
    let end = after
        .checked_add(returned)
        .ok_or_else(|| problem("The directory page position overflowed"))?;
    let expected = PAGE_SIZE.min(state.matched.saturating_sub(after));
    if returned != expected || has_more != (end < state.matched) {
        return Err(uncertain(
            "The durable directory page differs from its completed index metadata",
        ));
    }
    let snapshot = snapshot_cursor(request, state)?;
    let next = has_more.then(|| page_cursor(&request.materialization, digest, end));
    Ok(serde_json::json!({
        "ready": true,
        "phase": MaterializationPhase::Completed.as_str(),
        "cursor": serde_json::Value::Null,
        "directory": request.directory_identity,
        "filter": request.filter,
        "offset": after,
        "snapshot": snapshot,
        "entries": entries,
        "indexed": state.scanned,
        "total": state.matched,
        "next": next,
    }))
}

fn validate_metadata(
    connection: &Connection,
    request: &MaterializationRequest,
    state: &MaterializationState,
    digest: &str,
) -> Result<()> {
    let retained = connection
        .query_row(
            "SELECT schema_version,workspace,store,materialization,
                    directory_identity,filter,matched,digest
             FROM metadata WHERE singleton=1",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            },
        )
        .map_err(problem)?;
    if retained.0 != i64::from(SCHEMA_VERSION)
        || retained.1 != request.workspace
        || retained.2 != request.store
        || retained.3 != request.materialization
        || retained.4 != request.directory_identity
        || retained.5 != request.filter
        || retained.6 != state.matched.to_string()
        || retained.7 != digest
    {
        return Err(uncertain(
            "The directory index metadata differs from its completed state",
        ));
    }
    Ok(())
}

fn validate_query(
    request: &MaterializationRequest,
    relative: &str,
    expected_directory: &str,
    filter: &str,
) -> Result<()> {
    if request.relative != relative
        || request.filter != filter
        || (!expected_directory.is_empty()
            && request.directory_identity != expected_directory)
    {
        return Err(problem(
            "The directory cursor belongs to a different project query",
        ));
    }
    Ok(())
}

fn index_cursor(materialization: &str) -> String {
    format!("v1:index:{materialization}")
}

fn decode_index_cursor(cursor: &str) -> Result<Option<&str>> {
    let Some(materialization) = cursor.strip_prefix("v1:index:") else {
        return Ok(None);
    };
    validate_hex_identity(materialization, "directory indexing cursor")?;
    if cursor != index_cursor(materialization) {
        return Err(problem("The directory indexing cursor is not canonical"));
    }
    Ok(Some(materialization))
}

fn page_cursor(materialization: &str, digest: &str, position: u64) -> String {
    format!("v1:page:{materialization}:{digest}:{position}")
}

fn decode_page_cursor(cursor: &str) -> Result<(&str, &str, u64)> {
    let mut fields = cursor.split(':');
    let (Some("v1"), Some("page"), Some(materialization), Some(digest), Some(position), None) = (
        fields.next(),
        fields.next(),
        fields.next(),
        fields.next(),
        fields.next(),
        fields.next(),
    ) else {
        return Err(problem("The directory page cursor is malformed"));
    };
    validate_hex_identity(materialization, "directory materialization")?;
    validate_digest(digest, "directory page")?;
    let position = position
        .parse::<u64>()
        .map_err(|_| problem("The directory page cursor position is malformed"))?;
    if position == 0 || cursor != page_cursor(materialization, digest, position) {
        return Err(problem("The directory page cursor is not canonical"));
    }
    Ok((materialization, digest, position))
}

fn snapshot_cursor(
    request: &MaterializationRequest,
    state: &MaterializationState,
) -> Result<String> {
    let digest = state
        .digest
        .as_deref()
        .ok_or_else(|| uncertain("The completed directory index has no snapshot digest"))?;
    Ok(format!(
        "v1:snapshot:{}:{digest}:{}",
        request.materialization, state.matched
    ))
}
