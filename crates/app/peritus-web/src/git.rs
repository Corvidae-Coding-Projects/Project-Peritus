//! Git operations use argument vectors, exact pathspecs, and an explicit repository binding.

use crate::{
    error::{Result, problem, uncertain},
    files,
    state::{App, Project},
};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

mod workflows;

async fn run(directory: &Path, args: &[String]) -> Result<String> {
    execute(directory, args, false, None).await
}
#[cfg(test)]
async fn run_effect(directory: &Path, args: &[String]) -> Result<String> {
    execute(directory, args, true, None).await
}
async fn run_effect_owned(
    directory: &Path,
    args: &[String],
    app: Option<&App>,
    operation: &str,
) -> Result<String> {
    execute(directory, args, true, app.map(|app| (app, operation))).await
}
async fn execute(
    directory: &Path,
    args: &[String],
    effect: bool,
    owner: Option<(&App, &str)>,
) -> Result<String> {
    let mut command_args = vec!["--no-pager".into()];
    command_args.extend_from_slice(args);
    let command = if let Some((app, operation)) = owner {
        crate::processes::ManagedCommand::start(
            app,
            &format!("git:{operation}"),
            directory,
            "git".into(),
            command_args,
            false,
        )?
    } else {
        let root = directory.to_path_buf();
        tokio::task::spawn_blocking(move || {
            crate::processes::ManagedCommand::temporary(&root, "git".into(), command_args)
        })
        .await
        .map_err(problem)??
    };
    let (state, stdout, stderr) = crate::processes::wait(command).await?;
    project_output(state, &stdout, &stderr, args, effect)
}
fn project_output(
    state: peritus_product_runner::PreviewProcessState,
    stdout: &str,
    stderr: &str,
    args: &[String],
    effect: bool,
) -> Result<String> {
    use peritus_product_runner::PreviewProcessState;
    if matches!(
        state,
        PreviewProcessState::Cancelled
            | PreviewProcessState::TimedOut
            | PreviewProcessState::Indeterminate
    ) {
        return Err(uncertain(
            "Git stopped without confirmed completion; inspect local and remote repository state before another mutation",
        ));
    }
    if state != PreviewProcessState::Succeeded {
        let detail = stderr;
        // A remote can accept a push before its acknowledgement is lost. Fetch and
        // pull may also update refs before reporting a later failure.
        if effect
            && args.first().is_some_and(|arg| ["fetch", "pull", "push"].contains(&arg.as_str()))
        {
            return Err(uncertain(format!(
                "{detail}\nGit did not confirm completion. Inspect local and remote state before another mutation."
            )));
        }
        return Err(problem(detail));
    }
    Ok(format!("{stdout}{}", if effect { stderr } else { "" }))
}
async fn repository(project: &Project) -> Result<PathBuf> {
    let selected = files::resolve(&project.root, &project.repository.to_string_lossy())?;
    let toplevel = run(&selected, &["rev-parse".into(), "--show-toplevel".into()]).await
        .map_err(|error| problem(format!("No working Git repository at the selected location. Set a repository directory in Git settings. {error}")))?;
    let root = PathBuf::from(toplevel.trim()).canonicalize()?;
    if !root.starts_with(&project.root) {
        return Err(problem("The selected Git repository belongs outside this project root"));
    }
    Ok(root)
}
pub async fn status(project: &Project) -> Result<Value> {
    let root = repository(project).await?;
    let output = run(
        &root,
        &["status".into(), "--short".into(), "-z".into(), "--untracked-files=all".into()],
    )
    .await?;
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
    let branch = run(&root, &["branch".into(), "--show-current".into()]).await?;
    let remotes = run(&root, &["remote".into(), "-v".into()]).await?;
    let (branches, remote_details) = workflows::inventory(&root).await?;
    Ok(json!({"root":root,"branch":branch.trim(),"changes":changes,"remotes":remotes,
        "branches":branches,"remoteDetails":remote_details}))
}
pub async fn action(
    app: &App,
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
        return Ok(json!({"output":workflows::action(&root, kind, input, Some(app)).await?}));
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
    Ok(
        json!({"output":run_effect_owned(&root, &args, Some(app), input["operation"].as_str().unwrap_or("")).await?}),
    )
}
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

pub async fn recover(app: &App, operation: &str, input: &Value) -> Result<Option<Value>> {
    // Check custody before the command snapshot: a live preparer may publish the binding
    // and finish between these observations. Once custody ends, its binding is durable.
    if app.owned_operations.lock().map_err(problem)?.contains(operation) {
        return Ok(None);
    }
    if !app.snapshot()?.commands.contains_key(&format!("git:{operation}")) {
        // No binding was registered, so no Git mutation could have reached the process gateway.
        return Ok(Some(
            json!({"error":"Git stopped before command registration; retry is safe", "submitted":false, "retryable":true}),
        ));
    }
    let command = crate::processes::ManagedCommand::get(app, &format!("git:{operation}"))?;
    if command.observe()?.state() == peritus_product_runner::PreviewProcessState::Running {
        return Ok(None);
    }
    let (state, stdout, stderr) = crate::processes::wait(command).await?;
    let args = vec![input["action"].as_str().unwrap_or("").to_owned()];
    match project_output(state, &stdout, &stderr, &args, true) {
        Ok(output) => Ok(Some(json!({"output":output,"recovered":true}))),
        Err(error) if error.1 => Err(error),
        Err(error) => Ok(Some(json!({"error":error.0,"recovered":true}))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn network_effect_errors_are_not_replayed_as_known_failures() {
        let directory = tempfile::tempdir().unwrap();
        let error = run_effect(directory.path(), &["push".into()]).await.unwrap_err();
        assert!(error.1);
        let query = run(directory.path(), &["status".into()]).await.unwrap_err();
        assert!(!query.1);
    }
    #[test]
    fn ignores_literal_names_not_patterns() {
        assert_eq!(ignore_pattern(Path::new("a[1]*.txt"), false), "/a\\[1\\]\\*.txt");
        assert_eq!(ignore_pattern(Path::new("build output"), true), "/build\\ output/");
    }
}
