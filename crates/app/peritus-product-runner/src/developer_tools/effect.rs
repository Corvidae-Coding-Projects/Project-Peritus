//! Shared bounded process and filesystem effect mechanics.

use std::{
    collections::VecDeque,
    fs,
    io::{Read, Write as _},
    path::Path,
};

use peritus_agent::DeveloperLoopError;

use super::path::tool;

const MAX_OUTPUT_BYTES: usize = 512 * 1024;
const OUTPUT_HEAD_BYTES: usize = MAX_OUTPUT_BYTES / 2;
const OUTPUT_TAIL_BYTES: usize = MAX_OUTPUT_BYTES - OUTPUT_HEAD_BYTES;
const TRUNCATION_MARKER: &str = "\n[output truncated]\n";

pub(super) fn reject_destructive_command(
    program: &str,
    args: &[String],
) -> Result<(), DeveloperLoopError> {
    let executable =
        Path::new(program).file_name().and_then(|name| name.to_str()).unwrap_or(program);
    let direct_delete = matches!(executable, "rm" | "unlink" | "rmdir");
    let git_clean = executable == "git" && args.first().is_some_and(|arg| arg == "clean");
    let find_delete = executable == "find" && args.iter().any(|arg| arg == "-delete");
    if direct_delete || git_clean || find_delete {
        return Err(tool(
            "destructive commands are not available through run_command; inspect the exact target and use workspace_remove for an intentional regular-file or empty-directory deletion",
        ));
    }
    Ok(())
}

pub(super) fn atomic_write(path: &Path, content: &[u8]) -> Result<(), DeveloperLoopError> {
    let temporary = path.with_extension("peritus-new");
    let mut file = fs::File::create(&temporary).map_err(|error| tool(error.to_string()))?;
    file.write_all(content).map_err(|error| tool(error.to_string()))?;
    file.sync_all().map_err(|error| tool(error.to_string()))?;
    #[cfg(windows)]
    if path.is_file() {
        fs::remove_file(path).map_err(|error| tool(error.to_string()))?;
    }
    fs::rename(temporary, path).map_err(|error| tool(error.to_string()))
}

pub(super) fn atomic_write_if_changed(
    path: &Path,
    content: &[u8],
) -> Result<bool, DeveloperLoopError> {
    if path.is_file() && fs::read(path).map_err(|error| tool(error.to_string()))? == content {
        return Ok(false);
    }
    atomic_write(path, content)?;
    Ok(true)
}

pub(super) fn limit(value: &str) -> String {
    if value.len() <= MAX_OUTPUT_BYTES {
        value.to_owned()
    } else {
        format!("{}\n[output truncated]", &value[..value.floor_char_boundary(MAX_OUTPUT_BYTES)])
    }
}

pub(super) fn drain_bounded(mut reader: impl Read) -> std::io::Result<String> {
    let mut head = Vec::with_capacity(OUTPUT_HEAD_BYTES);
    let mut tail = VecDeque::with_capacity(OUTPUT_TAIL_BYTES);
    let mut buffer = [0_u8; 8 * 1024];
    let mut observed = 0_usize;
    loop {
        let bytes = reader.read(&mut buffer)?;
        if bytes == 0 {
            break;
        }
        observed = observed.saturating_add(bytes);
        let head_bytes = (OUTPUT_HEAD_BYTES - head.len()).min(bytes);
        head.extend_from_slice(&buffer[..head_bytes]);
        retain_tail(&mut tail, &buffer[head_bytes..bytes]);
    }
    let tail = tail.make_contiguous();
    if observed <= MAX_OUTPUT_BYTES {
        head.extend_from_slice(tail);
        return Ok(String::from_utf8_lossy(&head).into_owned());
    }
    Ok(format!(
        "{}{}{}",
        String::from_utf8_lossy(&head),
        TRUNCATION_MARKER,
        String::from_utf8_lossy(tail),
    ))
}

fn retain_tail(tail: &mut VecDeque<u8>, bytes: &[u8]) {
    if bytes.len() >= OUTPUT_TAIL_BYTES {
        tail.clear();
        tail.extend(&bytes[bytes.len() - OUTPUT_TAIL_BYTES..]);
        return;
    }
    let excess = tail.len().saturating_add(bytes.len()).saturating_sub(OUTPUT_TAIL_BYTES);
    tail.drain(..excess);
    tail.extend(bytes);
}
