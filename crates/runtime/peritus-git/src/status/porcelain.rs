//! Strict parser for the fixed porcelain-v2 invocation used by this crate.

use crate::status::{
    ChangeCode, EntryModes, MAX_STATUS_BYTES, MAX_STATUS_ENTRIES, MAX_STATUS_PATH_BYTES,
    StatusEntry, StatusKind, SubmoduleState,
};
use crate::{CommitId, ErrorKind, GitError, ObjectFormat, ObjectId, Operation, RecoveryClass};

pub(super) struct ParsedStatus {
    pub(super) head: CommitId,
    pub(super) detached: bool,
    pub(super) entries: Vec<StatusEntry>,
}

pub(super) fn parse(input: &[u8], format: ObjectFormat) -> Result<ParsedStatus, GitError> {
    if input.is_empty() || input.last() != Some(&0) {
        return Err(protocol("porcelain-v2 output is empty or lacks a final NUL"));
    }
    let mut records = input[..input.len() - 1].split(|byte| *byte == 0);
    let mut head = None;
    let mut saw_branch_oid = false;
    let mut detached = None;
    let mut entries = Vec::new();
    while let Some(record) = records.next() {
        if record.is_empty() {
            return Err(protocol("porcelain-v2 contains an empty record"));
        }
        match record[0] {
            b'#' => parse_header(record, format, &mut head, &mut saw_branch_oid, &mut detached)?,
            b'1' => entries.push(parse_ordinary(record, format)?),
            b'2' => {
                let original =
                    records.next().ok_or_else(|| protocol("rename lacks original path"))?;
                entries.push(parse_renamed(record, original, format)?);
            }
            b'u' => entries.push(parse_unmerged(record, format)?),
            b'?' => entries.push(simple_path(record, StatusKind::Untracked)?),
            b'!' => entries.push(simple_path(record, StatusKind::Ignored)?),
            _ => return Err(protocol("porcelain-v2 contains an unsupported record type")),
        }
        if !crate::verified::status_shape_within_bounds(
            input.len(),
            entries.len(),
            MAX_STATUS_BYTES,
            MAX_STATUS_ENTRIES,
        ) {
            return Err(protocol("porcelain-v2 contains too many entries"));
        }
    }
    if !saw_branch_oid {
        return Err(protocol("porcelain-v2 lacks branch.oid"));
    }
    let head = head.ok_or_else(|| protocol("unborn HEAD is unsupported for managed worktrees"))?;
    let detached = detached.ok_or_else(|| protocol("porcelain-v2 lacks branch.head"))?;
    Ok(ParsedStatus { head, detached, entries })
}

fn parse_header(
    record: &[u8],
    format: ObjectFormat,
    head: &mut Option<CommitId>,
    saw_branch_oid: &mut bool,
    detached: &mut Option<bool>,
) -> Result<(), GitError> {
    let value = record.strip_prefix(b"# ").ok_or_else(|| protocol("malformed status header"))?;
    if let Some(object) = value.strip_prefix(b"branch.oid ") {
        if *saw_branch_oid {
            return Err(protocol("duplicate branch.oid header"));
        }
        *saw_branch_oid = true;
        if object == b"(initial)" {
            return Ok(());
        }
        let object =
            std::str::from_utf8(object).map_err(|_| protocol("non-UTF-8 branch object"))?;
        *head = Some(CommitId::checked(ObjectId::parse(format, object, Operation::Status)?));
    } else if let Some(branch) = value.strip_prefix(b"branch.head ") {
        if detached.is_some() || branch.is_empty() || branch.iter().any(u8::is_ascii_control) {
            return Err(protocol("malformed or duplicate branch.head header"));
        }
        *detached = Some(branch == b"(detached)");
    } else if value.starts_with(b"branch.upstream ") || value.starts_with(b"branch.ab ") {
        // These bounded informational headers do not affect the canonical typed entry set.
    } else {
        return Err(protocol("unsupported porcelain-v2 status header"));
    }
    Ok(())
}

fn parse_ordinary(record: &[u8], format: ObjectFormat) -> Result<StatusEntry, GitError> {
    let fields = fields(record, 9)?;
    if fields[0] != b"1" {
        return Err(protocol("ordinary record discriminator mismatch"));
    }
    let (index, worktree) = changes(fields[1])?;
    let submodule = submodule(fields[2])?;
    let modes =
        EntryModes { head: mode(fields[3])?, index: mode(fields[4])?, worktree: mode(fields[5])? };
    object_or_zero(fields[6], format)?;
    object_or_zero(fields[7], format)?;
    Ok(StatusEntry {
        path: path(fields[8])?,
        kind: StatusKind::Ordinary { index, worktree, submodule, modes },
    })
}

fn parse_renamed(
    record: &[u8],
    original: &[u8],
    format: ObjectFormat,
) -> Result<StatusEntry, GitError> {
    let fields = fields(record, 10)?;
    if fields[0] != b"2" {
        return Err(protocol("rename record discriminator mismatch"));
    }
    let (index, worktree) = changes(fields[1])?;
    let submodule = submodule(fields[2])?;
    let modes =
        EntryModes { head: mode(fields[3])?, index: mode(fields[4])?, worktree: mode(fields[5])? };
    object_or_zero(fields[6], format)?;
    object_or_zero(fields[7], format)?;
    let score_field = fields[8];
    if score_field.len() < 2 || !matches!(score_field[0], b'R' | b'C') {
        return Err(protocol("rename score is malformed"));
    }
    let score = decimal(&score_field[1..])?;
    if score > 100 {
        return Err(protocol("rename score exceeds 100"));
    }
    Ok(StatusEntry {
        path: path(fields[9])?,
        kind: StatusKind::Renamed {
            index,
            worktree,
            submodule,
            modes,
            score,
            original_path: path(original)?,
        },
    })
}

fn parse_unmerged(record: &[u8], format: ObjectFormat) -> Result<StatusEntry, GitError> {
    let fields = fields(record, 11)?;
    if fields[0] != b"u" {
        return Err(protocol("unmerged record discriminator mismatch"));
    }
    let (index, worktree) = changes(fields[1])?;
    if index != ChangeCode::Unmerged && worktree != ChangeCode::Unmerged {
        return Err(protocol("unmerged record lacks an unmerged change code"));
    }
    let submodule = submodule(fields[2])?;
    let ancestor_mode = mode(fields[3])?;
    let ours_mode = mode(fields[4])?;
    let theirs_mode = mode(fields[5])?;
    let worktree_mode = mode(fields[6])?;
    object_or_zero(fields[7], format)?;
    object_or_zero(fields[8], format)?;
    object_or_zero(fields[9], format)?;
    Ok(StatusEntry {
        path: path(fields[10])?,
        kind: StatusKind::Unmerged {
            submodule,
            ancestor_mode,
            ours_mode,
            theirs_mode,
            worktree_mode,
        },
    })
}

fn simple_path(record: &[u8], kind: StatusKind) -> Result<StatusEntry, GitError> {
    if record.len() < 3 || record[1] != b' ' {
        return Err(protocol("simple status record is malformed"));
    }
    Ok(StatusEntry { path: path(&record[2..])?, kind })
}

fn fields(record: &[u8], count: usize) -> Result<Vec<&[u8]>, GitError> {
    let fields: Vec<_> = record.splitn(count, |byte| *byte == b' ').collect();
    if fields.len() != count || fields.iter().any(|field| field.is_empty()) {
        return Err(protocol("porcelain-v2 record has the wrong field count"));
    }
    Ok(fields)
}

fn changes(value: &[u8]) -> Result<(ChangeCode, ChangeCode), GitError> {
    if value.len() != 2 {
        return Err(protocol("porcelain-v2 change pair is malformed"));
    }
    Ok((change(value[0])?, change(value[1])?))
}

fn change(value: u8) -> Result<ChangeCode, GitError> {
    match value {
        b'.' => Ok(ChangeCode::Unmodified),
        b'A' => Ok(ChangeCode::Added),
        b'M' => Ok(ChangeCode::Modified),
        b'D' => Ok(ChangeCode::Deleted),
        b'R' => Ok(ChangeCode::Renamed),
        b'C' => Ok(ChangeCode::Copied),
        b'T' => Ok(ChangeCode::TypeChanged),
        b'U' => Ok(ChangeCode::Unmerged),
        _ => Err(protocol("porcelain-v2 change code is unsupported")),
    }
}

fn submodule(value: &[u8]) -> Result<SubmoduleState, GitError> {
    match value {
        b"N..." => Ok(SubmoduleState {
            is_submodule: false,
            commit_changed: false,
            modified_content: false,
            untracked_content: false,
        }),
        [b'S', commit, modified, untracked]
            if matches!(commit, b'.' | b'C')
                && matches!(modified, b'.' | b'M')
                && matches!(untracked, b'.' | b'U') =>
        {
            Ok(SubmoduleState {
                is_submodule: true,
                commit_changed: *commit == b'C',
                modified_content: *modified == b'M',
                untracked_content: *untracked == b'U',
            })
        }
        _ => Err(protocol("porcelain-v2 submodule state is malformed")),
    }
}

fn mode(value: &[u8]) -> Result<u32, GitError> {
    if value.len() != 6 || !value.iter().all(|byte| matches!(byte, b'0'..=b'7')) {
        return Err(protocol("porcelain-v2 file mode is malformed"));
    }
    let value = std::str::from_utf8(value).map_err(|_| protocol("file mode is not ASCII"))?;
    u32::from_str_radix(value, 8).map_err(|_| protocol("file mode cannot be represented"))
}

fn object_or_zero(value: &[u8], format: ObjectFormat) -> Result<(), GitError> {
    if value.len() != format.hex_len() {
        return Err(protocol("porcelain-v2 object ID has the wrong length"));
    }
    if value.iter().all(|byte| *byte == b'0') {
        return Ok(());
    }
    let value = std::str::from_utf8(value).map_err(|_| protocol("object ID is not ASCII"))?;
    ObjectId::parse(format, value, Operation::Status).map(|_| ())
}

fn decimal(value: &[u8]) -> Result<u8, GitError> {
    if value.is_empty() || !value.iter().all(u8::is_ascii_digit) {
        return Err(protocol("porcelain-v2 decimal is malformed"));
    }
    let value = std::str::from_utf8(value).map_err(|_| protocol("decimal is not ASCII"))?;
    value.parse().map_err(|_| protocol("decimal cannot be represented"))
}

fn path(value: &[u8]) -> Result<String, GitError> {
    if value.is_empty()
        || value.len() > MAX_STATUS_PATH_BYTES
        || value[0] == b'/'
        || value.iter().any(|byte| *byte == 0 || byte.is_ascii_control())
    {
        return Err(protocol("porcelain-v2 path is invalid or exceeds bounds"));
    }
    let value = std::str::from_utf8(value).map_err(|_| protocol("status path is not UTF-8"))?;
    if value.split('/').any(|component| component.is_empty() || matches!(component, "." | "..")) {
        return Err(protocol("porcelain-v2 path is not canonical relative form"));
    }
    Ok(value.to_owned())
}

fn protocol(detail: &'static str) -> GitError {
    GitError::new(ErrorKind::GitProtocol, Operation::Status, RecoveryClass::Quarantine, detail)
}

#[cfg(test)]
mod tests {
    use super::parse;
    use crate::{ObjectFormat, StatusKind};

    const HEAD: &str = "0123456789012345678901234567890123456789";

    #[test]
    fn parses_headers_and_all_simple_record_classes() {
        let input = format!(
            "# branch.oid {HEAD}\0# branch.head (detached)\01 M. N... 100644 100644 100644 {HEAD} {HEAD} tracked\0? new file\0! ignored\0"
        );
        let status = parse(input.as_bytes(), ObjectFormat::Sha1).expect("status");
        assert!(status.detached);
        assert_eq!(status.entries.len(), 3);
        assert!(matches!(status.entries[0].kind(), StatusKind::Ordinary { .. }));
        assert!(matches!(status.entries[1].kind(), StatusKind::Untracked));
        assert!(matches!(status.entries[2].kind(), StatusKind::Ignored));
    }

    #[test]
    fn rejects_unterminated_and_malformed_records() {
        assert!(parse(b"# branch.oid deadbeef", ObjectFormat::Sha1).is_err());
        let input = format!("# branch.oid {HEAD}\0# branch.head (detached)\0? ../escape\0");
        assert!(parse(input.as_bytes(), ObjectFormat::Sha1).is_err());
    }
}
