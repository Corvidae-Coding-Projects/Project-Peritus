//! Shared developer-tool effect mechanics.

use std::{fs, io::Write as _, path::Path};

use peritus_agent::DeveloperLoopError;

use super::path::tool;

#[cfg(test)]
mod tests;

pub(super) fn reject_destructive_command(
    program: &str,
    args: &[String],
) -> Result<(), DeveloperLoopError> {
    let executable =
        Path::new(program).file_name().and_then(|name| name.to_str()).unwrap_or(program);
    let executable = executable.to_ascii_lowercase();
    let executable = canonical_program(&executable);
    let direct_delete = destructive_program(executable);
    let git_clean = executable == "git" && args.first().is_some_and(|arg| arg == "clean");
    let find_delete = executable == "find"
        && args.iter().any(|arg| arg == "-delete" || destructive_program_arg(arg));
    let wrapper_delete = matches!(executable, "sudo" | "env" | "xargs")
        && args.iter().any(|arg| destructive_program_arg(arg));
    let shell_delete = matches!(
        executable,
        "sh" | "bash" | "dash" | "zsh" | "fish" | "cmd" | "powershell" | "pwsh"
    ) && args.iter().any(|arg| script_contains_delete(arg));
    if direct_delete || git_clean || find_delete || wrapper_delete || shell_delete {
        return Err(tool(
            "destructive commands are not available through run_command; inspect the exact target and use workspace_remove for an intentional single-target deletion, or its typed recursive mode when the governing user request explicitly authorizes that exact tree",
        ));
    }
    Ok(())
}

fn canonical_program(program: &str) -> &str {
    program.strip_suffix(".exe").unwrap_or(program)
}

fn destructive_program(program: &str) -> bool {
    matches!(
        canonical_program(program),
        "rm" | "unlink" | "rmdir" | "del" | "erase" | "rd" | "remove-item"
    )
}

fn destructive_program_arg(argument: &str) -> bool {
    Path::new(argument)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| destructive_program(&name.to_ascii_lowercase()))
}

fn script_contains_delete(script: &str) -> bool {
    let words = script
        .split(|character: char| {
            character.is_whitespace()
                || matches!(character, ';' | '|' | '&' | '(' | ')' | '{' | '}')
        })
        .map(|word| word.trim_matches(|character| matches!(character, '\'' | '"')))
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    words.iter().any(|word| destructive_program_arg(word))
        || words.windows(2).any(|pair| {
            pair[0].eq_ignore_ascii_case("git") && pair[1].eq_ignore_ascii_case("clean")
        })
        || words.iter().any(|word| *word == "-delete")
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
