//! Branch and remote workflows preserve Git's ordinary dirty-worktree protections.

use super::{run, run_effect};
use crate::{
    error::{Result, problem},
    state::{App, OperationOwner},
};
use serde_json::Value;
use std::path::Path;

mod inventory;

pub(crate) use inventory::{
    inventory, owner_argument as inventory_owner_argument, run_owner as run_inventory_owner,
};

async fn branch_name(root: &Path, name: &str) -> Result<String> {
    if name.is_empty()
        || name.starts_with('-')
        || name.contains("@{")
        || name.chars().any(char::is_control)
    {
        return Err(problem("Enter a literal branch name, such as feature/editor"));
    }
    let checked = run(root, &["check-ref-format".into(), "--branch".into(), name.into()]).await?;
    if checked.trim() != name {
        return Err(problem("Use a literal branch name"));
    }
    Ok(name.into())
}

async fn remote_name(root: &Path, name: &str) -> Result<String> {
    if name.is_empty() || name.starts_with('-') || name.chars().any(char::is_control) {
        return Err(problem("Enter a remote name, such as origin"));
    }
    run(root, &["check-ref-format".into(), format!("refs/remotes/{name}/probe")]).await?;
    Ok(name.into())
}

fn remote_url(url: &str) -> Result<String> {
    if url.trim().is_empty()
        || url.starts_with('-')
        || url.contains("::")
        || url.chars().any(char::is_control)
    {
        return Err(problem("Enter an HTTP, SSH, Git, or local repository address"));
    }
    if let Some((scheme, _)) = url.split_once("://")
        && !["https", "http", "ssh", "git", "file"].contains(&scheme)
    {
        return Err(problem("This remote URL scheme is not supported"));
    }
    Ok(url.into())
}

pub(super) async fn action(
    app: &App,
    owner: &OperationOwner,
    root: &Path,
    kind: &str,
    input: &Value,
) -> Result<Value> {
    let field = |key: &str| input[key].as_str().unwrap_or("");
    let args = match kind {
        "branch-create" => {
            let mut args =
                vec!["switch".into(), "--create".into(), branch_name(root, field("branch")).await?];
            if !field("start").is_empty() {
                let commit = run(
                    root,
                    &[
                        "rev-parse".into(),
                        "--verify".into(),
                        "--end-of-options".into(),
                        format!("{}^{{commit}}", field("start")),
                    ],
                )
                .await?;
                args.push(commit.trim().into());
            }
            args
        }
        "branch-switch" => {
            let target = field("branch");
            if let Some(name) = target.strip_prefix("refs/remotes/") {
                branch_name(root, name).await?;
                vec!["switch".into(), "--track".into(), target.into()]
            } else {
                let name = target.strip_prefix("refs/heads/").unwrap_or(target);
                vec!["switch".into(), "--no-guess".into(), branch_name(root, name).await?]
            }
        }
        "branch-rename" => vec![
            "branch".into(),
            "--move".into(),
            branch_name(root, field("branch")).await?,
            branch_name(root, field("name")).await?,
        ],
        "branch-delete" if input["confirmed"] == true => {
            vec!["branch".into(), "--delete".into(), branch_name(root, field("branch")).await?]
        }
        "remote-add" => vec![
            "remote".into(),
            "add".into(),
            remote_name(root, field("remote")).await?,
            remote_url(field("url"))?,
        ],
        "remote-url" => {
            let mut args = vec!["remote".into(), "set-url".into()];
            if input["pushUrl"] == true {
                args.push("--push".into());
            }
            args.extend([remote_name(root, field("remote")).await?, remote_url(field("url"))?]);
            args
        }
        "remote-rename" => vec![
            "remote".into(),
            "rename".into(),
            remote_name(root, field("remote")).await?,
            remote_name(root, field("name")).await?,
        ],
        "remote-remove" if input["confirmed"] == true => {
            vec!["remote".into(), "remove".into(), remote_name(root, field("remote")).await?]
        }
        "fetch" => {
            let mut args = vec!["fetch".into(), "--prune".into()];
            args.push(if field("remote").is_empty() {
                "--all".into()
            } else {
                remote_name(root, field("remote")).await?
            });
            args
        }
        "push" | "pull" => {
            let mut args = vec![kind.into()];
            if kind == "pull" {
                args.push("--ff-only".into());
            }
            if input["setUpstream"] == true && kind == "push" {
                args.push("--set-upstream".into());
            }
            if !field("remote").is_empty() {
                args.push(remote_name(root, field("remote")).await?);
                if !field("branch").is_empty() {
                    let branch = branch_name(root, field("branch")).await?;
                    args.push(if kind == "push" {
                        format!("HEAD:refs/heads/{branch}")
                    } else {
                        branch
                    });
                }
            } else if !field("branch").is_empty() || input["setUpstream"] == true {
                return Err(problem("Select a remote for this branch"));
            }
            args
        }
        _ => return Err(problem("Unknown Git workflow or missing removal confirmation")),
    };
    run_effect(app, owner, root, &args).await
}

#[cfg(test)]
mod tests;
