//! Shared developer-tool effect mechanics.

use std::{fs, io::Write as _, path::Path};

use peritus_agent::DeveloperLoopError;

use super::path::tool;

const MAX_OUTPUT_BYTES: usize = 512 * 1024;

#[cfg(test)]
mod tests;

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
    let permissions = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => Some(metadata.permissions()),
        Ok(_) => return Err(tool("workspace replacement requires a regular file")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(tool(error.to_string())),
    };
    let parent = path.parent().ok_or_else(|| tool("workspace file has no parent"))?;
    let mut file =
        tempfile::NamedTempFile::new_in(parent).map_err(|error| tool(error.to_string()))?;
    file.write_all(content).map_err(|error| tool(error.to_string()))?;
    if let Some(permissions) = permissions {
        file.as_file().set_permissions(permissions).map_err(|error| tool(error.to_string()))?;
    }
    file.as_file().sync_all().map_err(|error| tool(error.to_string()))?;
    file.persist(path).map(|_| ()).map_err(|error| tool(error.to_string()))
}

pub(super) fn limit(value: &str) -> String {
    if value.len() <= MAX_OUTPUT_BYTES {
        value.to_owned()
    } else {
        format!("{}\n[output truncated]", &value[..value.floor_char_boundary(MAX_OUTPUT_BYTES)])
    }
}
