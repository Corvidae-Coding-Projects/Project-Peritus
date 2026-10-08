//! Single-owner admission for related gateway mutations.

use crate::{
    error::{Result, problem},
    state::{App, id},
};
use serde_json::Value;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Target {
    Field(&'static str),
    Configuration,
}

pub(super) fn workspace(app: &App, input: &Value) -> Result<()> {
    if input["workspace"].as_str() != Some(app.snapshot()?.identity.as_str()) {
        return Err(problem("This action belongs to another gateway workspace. Its original identity was preserved; reopen that workspace to reconcile it."));
    }
    Ok(())
}

fn scope(input: &Value) -> Result<(String, Option<Target>)> {
    let command = input["command"].as_str().unwrap_or("");
    if command == "control" && input["action"] == "stop" {
        // Cancellation has its own admission owner; an uncertain send cannot hide its stop control.
        Ok((format!("cancel:{}", input["session"].as_str().unwrap_or("")), None))
    } else if ["git", "ignore", "repository"].contains(&command) {
        Ok((
            format!("git:{}", input["project"].as_str().unwrap_or("")),
            Some(Target::Field("project")),
        ))
    } else if ["send", "control", "session-settings"].contains(&command) {
        Ok((
            format!("conversation:{}", input["session"].as_str().unwrap_or("")),
            Some(Target::Field("session")),
        ))
    } else if ["config", "preferences"].contains(&command) {
        Ok(("configuration".into(), Some(Target::Configuration)))
    } else {
        Ok((format!("operation:{}", id()?), None))
    }
}

fn same_target(record: &Value, input: &Value, target: Target) -> bool {
    match target {
        Target::Field(field) => record[field] == input[field],
        Target::Configuration => {
            let command = record["command"].as_str().unwrap_or("");
            ["config", "preferences"].contains(&command)
        }
    }
}

pub(super) async fn guard(
    app: &Arc<App>,
    input: &Value,
) -> Result<tokio::sync::OwnedMutexGuard<()>> {
    // Serialize related Git, conversation and configuration effects while unrelated targets stay live.
    let (resource_key, target) = scope(input)?;
    let resource_lock = app.lock(resource_key)?;
    let resource_guard = resource_lock.lock_owned().await;
    if let Some(target) = target {
        let scan_app=Arc::clone(app);
        let scan_input=input.clone();
        let conflict=tokio::task::spawn_blocking(move || -> Result<Option<String>> {
            let mut after = None;
            let mut snapshot = None;
            loop {
                let page = scan_app.pending_operations(after.as_deref(), snapshot.as_deref())?;
                snapshot.get_or_insert_with(|| page.snapshot.clone());
                if let Some(conflict)=page.operations.into_iter().find_map(|(id, retained)| {
                    (id != scan_input["operation"].as_str().unwrap_or("")
                        && retained["command"] != "daemon-request"
                        && same_target(&retained, &scan_input, target))
                        .then_some(id)
                }) { return Ok(Some(conflict)); }
                let Some(cursor) = page.cursor else { return Ok(None) };
                after = Some(cursor);
            }
        }).await.map_err(problem)??;
        if let Some(id) = conflict {
            let target = match target {
                Target::Field(field) => field,
                Target::Configuration => "configuration",
            };
            return Err(problem(format!(
                "Resolve original operation {id} before another change to {target}."
            )));
        }
    }
    Ok(resource_guard)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Options, Preferences};
    use axum::{Json, extract::State};
    use serde_json::json;
    use std::path::PathBuf;

    fn test_app(root: &std::path::Path) -> Arc<App> {
        Arc::new(
            App::open(
                Options {
                    port: 4173,
                    root: root.to_owned(),
                    assets: root.join("assets"),
                    config_file: root.join("config/webui.toml"),
                    state_file: root.join("state/workspace.json"),
                    daemon_config_root: root.join("config"),
                    product_state_root: root.join("product-state"),
                    daemon_config: None,
                    endpoint: None,
                    cli: PathBuf::from("peritus"),
                },
                4173,
            )
            .expect("test application"),
        )
    }

    #[tokio::test]
    async fn unresolved_configuration_receipt_owns_only_configuration_admission() {
        let root = tempfile::tempdir().expect("root");
        let app = test_app(root.path());
        app.record_operation(
            "config-one".into(),
            json!({"operation":"config-one","command":"config","text":"theme = 'nixie'"}),
        )
        .expect("unresolved configuration operation");

        let Err(blocked) = super::super::action(
            State(Arc::clone(&app)),
            Json(json!({
                "operation":"preferences-one",
                "command":"preferences",
                "preferences":Preferences::default()
            })),
        )
        .await
        else {
            panic!("a sibling configuration write was admitted");
        };
        assert!(blocked.0.contains("config-one"));
        assert!(blocked.0.contains("configuration"));

        let unrelated = super::super::action(
            State(Arc::clone(&app)),
            Json(json!({
                "operation":"open-one",
                "command":"open-project",
                "root":root.path()
            })),
        )
        .await
        .expect("unrelated action remains available");
        assert!(unrelated.0["id"].is_string());

        app.settle_operation("config-one", json!({"theme":"nixie"}))
        .expect("settle original operation");
        let accepted = super::super::action(
            State(app),
            Json(json!({
                "operation":"preferences-two",
                "command":"preferences",
                "preferences":Preferences::default()
            })),
        )
        .await
        .expect("configuration admission resumes after settlement");
        assert!(accepted.0["config"].is_string());
    }
}
