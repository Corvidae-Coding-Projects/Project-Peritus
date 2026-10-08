//! Git operations use argument vectors, exact pathspecs, and an explicit repository binding.

use crate::{
    error::{Result, problem},
    files,
    state::{App, OperationOwner, Project},
};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

mod workflows;
pub(crate) mod effects;

async fn run(directory: &Path, args: &[String]) -> Result<String> {
    let mut command = tokio::process::Command::new("git");
    command
        .current_dir(directory)
        .arg("--no-pager")
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .kill_on_drop(true);
    let output = command.output().await?;
    if !output.status.success() {
        return Err(problem(String::from_utf8_lossy(&output.stderr)));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
async fn run_effect(
    app: &App,
    owner: &OperationOwner,
    directory: &Path,
    args: &[String],
) -> Result<Value> {
    effects::execute(app, owner, directory, args).await
}
async fn repository(project: &Project) -> Result<PathBuf> {
    let selected = files::resolve(&project.root, &project.repository.to_string_lossy())?;
    let toplevel = run(&selected, &["rev-parse".into(), "--show-toplevel".into()]).await
        .map_err(|_| problem("No working Git repository at the selected location. Set a repository directory in Git settings."))?;
    let root = PathBuf::from(toplevel.trim()).canonicalize()?;
    if !root.starts_with(&project.root) {
        return Err(problem("The selected Git repository belongs outside this project root"));
    }
    Ok(root)
}
pub async fn status(app: &App, project: &Project) -> Result<Value> {
    let root = repository(project).await?;
    let status_args = ["status".into(), "--short".into(), "-z".into(), "--untracked-files=all".into()];
    let branch_args = ["branch".into(), "--show-current".into()];
    let (output, branch, inventory) = tokio::try_join!(
        run(&root, &status_args),
        run(&root, &branch_args),
        workflows::inventory(app, &root, None, None),
    )?;
    let mut fields = output.split('\0').filter(|s| !s.is_empty());
    let mut changes = Vec::new();
    while let Some(entry) = fields.next() {
        if entry.len() < 4 {
            continue;
        }
        let code = &entry[..2];
        let path = &entry[3..];
        let from = if code.contains('R') || code.contains('C') { fields.next() } else { None };
        changes.push(json!({"code":code,"path":path,"from":from}));
    }
    Ok(json!({"root":root,"branch":branch.trim(),"changes":changes,"remotes":inventory.summary,
        "branches":inventory.branches,"remoteDetails":inventory.remotes,
        "inventoryCursor":inventory.cursor,"inventorySnapshot":inventory.snapshot}))
}
pub async fn inventory(
    app: &App,
    project: &Project,
    cursor: Option<&str>,
    snapshot: Option<&str>,
) -> Result<Value> {
    let root = repository(project).await?;
    let page = workflows::inventory(app, &root, cursor, snapshot).await?;
    Ok(json!({
        "branches":page.branches,
        "remoteDetails":page.remotes,
        "cursor":page.cursor,
        "snapshot":page.snapshot
    }))
}
pub async fn action(
    app: &App,
    owner: &OperationOwner,
    project: &Project,
    kind: &str,
    paths: Vec<String>,
    message: String,
    input: &Value,
) -> Result<Value> {
    let root = repository(project).await?;
    if kind.starts_with("branch-")
        || kind.starts_with("remote-")
        || ["fetch", "pull", "push"].contains(&kind)
    {
        return workflows::action(app, owner, &root, kind, input).await;
    }
    let mut args: Vec<String> = match kind {
        "add" => vec!["--literal-pathspecs".into(), "add".into()],
        "unstage" => vec!["--literal-pathspecs".into(), "restore".into(), "--staged".into()],
        "commit" if !message.trim().is_empty() => vec!["commit".into(), "-m".into(), message],
        "diff" => vec!["--literal-pathspecs".into(), "diff".into(), "--no-ext-diff".into()],
        "diff-staged" => vec![
            "--literal-pathspecs".into(),
            "diff".into(),
            "--no-ext-diff".into(),
            "--cached".into(),
        ],
        _ => return Err(problem("Choose a Git action; committing requires a message")),
    };
    if ["add", "unstage", "diff", "diff-staged"].contains(&kind) {
        args.push("--".into());
        if paths.is_empty() {
            args.push(".".into());
        } else {
            for path in paths {
                if Path::new(&path).is_absolute()
                    || Path::new(&path)
                        .components()
                        .any(|p| matches!(p, std::path::Component::ParentDir))
                {
                    return Err(problem("Git paths must be relative to the selected repository"));
                }
                args.push(path);
            }
        }
    }
    run_effect(app, owner, &root, &args).await
}

pub async fn output(
    app: &App,
    operation: &str,
    stream: &str,
    offset: u64,
) -> Result<Value> {
    let record = app
        .operation(operation)?
        .ok_or_else(|| problem("The original Git operation is not in this workspace ledger"))?;
    if record.input["command"] != "git" {
        return Err(problem("The selected operation does not own Git output"));
    }
    effects::output_page(app, operation, record.prepared.as_ref(), stream, offset).await
}

pub(crate) use effects::{
    cancel, command_argument, owner_argument, recover, run_command, run_owner, run_watchdog,
    watchdog_argument,
};
pub(crate) use workflows::{inventory_owner_argument, run_inventory_owner};
fn ignore_pattern(relative: &Path, directory: bool) -> String {
    let text = relative.to_string_lossy().replace('\\', "/");
    let mut escaped = String::from("/");
    for character in text.chars() {
        if matches!(character, '*' | '?' | '[' | ']' | '!' | '#' | '\\' | ' ') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    if directory {
        escaped.push('/');
    }
    escaped
}
pub async fn ignore(project: &Project, path: &str, apply: bool) -> Result<Value> {
    let root = repository(project).await?;
    let target = files::resolve(&project.root, path)?;
    let relative = target
        .strip_prefix(&root)
        .map_err(|_| problem("This file is outside the selected repository"))?;
    if relative.as_os_str().is_empty() {
        return Err(problem("Choose a file or subdirectory to ignore"));
    }
    let pattern = ignore_pattern(relative, target.is_dir());
    if pattern.contains(['\r', '\n']) {
        return Err(problem("Line breaks cannot be represented by this .gitignore action"));
    }
    let ignore_path = root.join(".gitignore");
    if ignore_path.exists() && ignore_path.canonicalize()?.parent() != Some(root.as_path()) {
        return Err(problem("The .gitignore link resolves outside the repository"));
    }
    let mut content = match std::fs::read_to_string(&ignore_path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };
    let present = content.lines().any(|line| line == pattern);
    if apply && !present {
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(&pattern);
        content.push('\n');
        crate::state::save(&ignore_path, content.as_bytes())?;
    }
    Ok(
        json!({"pattern":pattern,"present":present,"applied":apply,"file":ignore_path,"note":"Already tracked files remain tracked. This rule affects untracked files."}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ignores_literal_names_not_patterns() {
        assert_eq!(ignore_pattern(Path::new("a[1]*.txt"), false), "/a\\[1\\]\\*.txt");
        assert_eq!(ignore_pattern(Path::new("build output"), true), "/build\\ output/");
    }
}
