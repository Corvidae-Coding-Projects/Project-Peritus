//! Immutable digest-checked role files, never host paths chosen by a candidate.

use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use peritus_artifact_store::{ArtifactDigest, ArtifactStore};
use sha2::{Digest as _, Sha256};
use tempfile::TempDir;

use super::{ExecutableInput, ExecutionArgument, ProcessInputs, detail};

const EXECUTABLE_LIMIT: u64 = 256 * 1024 * 1024;
const INPUT_LIMIT: u64 = 16 * 1024 * 1024;
const TOTAL_INPUT_LIMIT: u64 = 64 * 1024 * 1024;

pub(super) struct Materialization {
    pub(super) directory: TempDir,
    pub(super) work: PathBuf,
    pub(super) executable: PathBuf,
    pub(super) arguments: Vec<String>,
    pub(super) files: Vec<MaterializedFile>,
}

pub(super) struct MaterializedFile {
    pub(super) path: PathBuf,
    digest: ArtifactDigest,
    size: u64,
    pub(super) executable: bool,
}

pub(super) fn prepare(
    artifacts: &ArtifactStore,
    root: &Path,
    executable: &ExecutableInput,
    inputs: &ProcessInputs,
) -> Result<Materialization, String> {
    if executable.size == 0 || executable.size > EXECUTABLE_LIMIT {
        return Err("RSI frozen executable size is outside the supported bound".into());
    }
    if executable.support_files.len() > 16 || executable.arguments.len() > 256 {
        return Err("RSI executable supplementary file or argument count exceeds its bound".into());
    }
    std::fs::create_dir_all(root).map_err(detail)?;
    let root = root.canonicalize().map_err(detail)?;
    let directory = tempfile::Builder::new().prefix("rollout-").tempdir_in(root).map_err(detail)?;
    let work = directory.path().join("work");
    let input = directory.path().join("inputs");
    std::fs::create_dir(&work).map_err(detail)?;
    std::fs::create_dir(&input).map_err(detail)?;
    for name in [".git", ".peritus", ".crosslink"] {
        std::fs::create_dir(work.join(name)).map_err(detail)?;
    }
    let path = input.join("executable");
    let file = copy_artifact(artifacts, executable.digest, &path, EXECUTABLE_LIMIT, true)?;
    if file.size != executable.size {
        return Err("RSI frozen executable artifact size differs from its descriptor".into());
    }
    let mut files = vec![file];
    let mut names = BTreeMap::new();
    let mut total = 0_u64;
    for (name, digest) in input_names(inputs) {
        let path = input.join(name);
        let file = copy_artifact(
            artifacts,
            digest,
            &path,
            INPUT_LIMIT.min(TOTAL_INPUT_LIMIT.saturating_sub(total)),
            false,
        )?;
        total = total.checked_add(file.size).ok_or("RSI input byte count overflow")?;
        names.insert(format!("{{{{{name}}}}}"), path);
        files.push(file);
    }
    for support in &executable.support_files {
        if !valid_name(&support.name) {
            return Err("RSI support file name must be a bounded flat ASCII identifier".into());
        }
        let path = input.join(format!("support-{}", support.name));
        let placeholder = format!("{{{{support:{}}}}}", support.name);
        if names.contains_key(&placeholder) {
            return Err("RSI frozen executable repeats a supplementary file name".into());
        }
        let file = copy_artifact(
            artifacts,
            support.digest,
            &path,
            INPUT_LIMIT.min(TOTAL_INPUT_LIMIT.saturating_sub(total)),
            false,
        )?;
        total = total.checked_add(file.size).ok_or("RSI input byte count overflow")?;
        names.insert(placeholder, path);
        files.push(file);
    }
    if total > TOTAL_INPUT_LIMIT {
        return Err("RSI role input files exceed their combined materialization bound".into());
    }
    let arguments = executable
        .arguments
        .iter()
        .map(|argument| {
            let (argument, resolve) = match argument {
                ExecutionArgument::Descriptor(value) => (value, true),
                ExecutionArgument::Literal(value) => (value, false),
            };
            if argument.len() > 16 * 1024 || argument.contains('\0') {
                return Err("RSI executable argument is not a bounded literal argv value".into());
            }
            if !resolve {
                return Ok(argument.clone());
            }
            if let Some(path) = names.get(argument) {
                return path
                    .to_str()
                    .map(str::to_owned)
                    .ok_or_else(|| "RSI materialized input path is not UTF-8".to_owned());
            }
            if argument.contains("{{") || argument.contains("}}") {
                return Err(
                    "RSI executable argument has an unavailable role input or invalid bound".into(),
                );
            }
            Ok(argument.clone())
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(Materialization { directory, work, executable: path, arguments, files })
}

fn input_names(inputs: &ProcessInputs) -> Vec<(&'static str, ArtifactDigest)> {
    match inputs {
        ProcessInputs::Candidate { public_task } => vec![("public_task", *public_task)],
        ProcessInputs::Evaluator {
            sealed_task,
            candidate_stdout,
            candidate_stderr,
            candidate_terminal,
        } => vec![
            ("sealed_task", *sealed_task),
            ("candidate_stdout", *candidate_stdout),
            ("candidate_stderr", *candidate_stderr),
            ("candidate_terminal", *candidate_terminal),
        ],
    }
}

fn copy_artifact(
    artifacts: &ArtifactStore,
    digest: ArtifactDigest,
    destination: &Path,
    maximum: u64,
    executable: bool,
) -> Result<MaterializedFile, String> {
    let mut reader = artifacts.open_read(digest).map_err(detail)?;
    let size = reader.metadata().size();
    if size > maximum {
        return Err("RSI immutable role file exceeds its materialization bound".into());
    }
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut output = options.open(destination).map_err(detail)?;
    let mut hasher = Sha256::new();
    // The store may itself have a small artifact ceiling; never request a chunk larger than
    // this object's validated size (one byte remains a valid EOF probe for an empty object).
    let chunk_bytes = usize::try_from(size.clamp(1, 16 * 1024))
        .map_err(|_| "RSI immutable artifact chunk exceeds address space")?;
    while let Some(chunk) = reader.read_chunk(chunk_bytes).map_err(detail)? {
        hasher.update(chunk.bytes());
        output.write_all(chunk.bytes()).map_err(detail)?;
    }
    output.sync_all().map_err(detail)?;
    if ArtifactDigest::new(hasher.finalize().into()) != digest {
        return Err("RSI artifact bytes changed during immutable materialization".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        output
            .set_permissions(std::fs::Permissions::from_mode(if executable {
                0o500
            } else {
                0o400
            }))
            .map_err(detail)?;
    }
    Ok(MaterializedFile { path: destination.to_path_buf(), digest, size, executable })
}

pub(super) fn revalidate(files: &[MaterializedFile]) -> Result<(), String> {
    for expected in files {
        let metadata = std::fs::symlink_metadata(&expected.path).map_err(detail)?;
        if !metadata.file_type().is_file() || metadata.len() != expected.size {
            return Err("RSI immutable role file changed before native launch".into());
        }
        let mut file =
            File::open(&expected.path).map_err(detail)?.take(expected.size.saturating_add(1));
        let mut hasher = Sha256::new();
        let mut observed = 0_u64;
        let mut bytes = [0_u8; 16 * 1024];
        loop {
            let count = file.read(&mut bytes).map_err(detail)?;
            if count == 0 {
                break;
            }
            observed = observed.checked_add(count as u64).ok_or("RSI file byte count overflow")?;
            hasher.update(&bytes[..count]);
        }
        if observed != expected.size
            || ArtifactDigest::new(hasher.finalize().into()) != expected.digest
        {
            return Err("RSI immutable role file digest changed before native launch".into());
        }
    }
    Ok(())
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name != "."
        && name != ".."
        && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"_-.".contains(&byte))
}
