//! Shared bounded process and filesystem effect mechanics.

use std::{fs, io::Write as _, path::Path};

use peritus_agent::DeveloperLoopError;

use super::path::tool;

const MAX_OUTPUT_BYTES: usize = 512 * 1024;

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
