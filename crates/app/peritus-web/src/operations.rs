//! Read-only reconciliation and explicit human acknowledgement of unprovable outcomes.
use crate::{
    config::Preferences,
    daemon,
    error::{Result, problem},
    files,
    state::{App, native_operation_belongs_to},
};
use serde_json::{Value, json};

pub fn validate_identity(identity: &str) -> Result<()> {
    if identity.is_empty() || !identity.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-') {
        return Err(problem("Operation identities must contain ASCII letters, digits, or hyphens"));
    }
    Ok(())
}

pub fn validate_recovery_identity(identity: &str) -> Result<()> {
    if identity.is_empty() {
        return Err(problem("Choose the original operation"));
    }
    Ok(())
}

pub fn pending_page(app: &App, after: Option<&str>, snapshot: Option<&str>) -> Result<Value> {
    let page = app.pending_operations(after, snapshot)?;
    let mut seen = std::collections::BTreeSet::new();
    let mut operations = Vec::new();
    for (retained_id, retained_input) in page.operations {
        let (id, input) = if retained_input["command"] == "daemon-request" {
            pending_parent(app, &retained_id, &retained_input)?
        } else {
            (retained_id, retained_input)
        };
        if seen.insert(id.clone()) {
            operations.push(json!({
                "operation":id,
                "command":input["command"],
                "session":input["session"],
                "project":input["project"],
                "path":input["path"]
            }));
        }
    }
    Ok(json!({
        "operations":operations,
        "cursor":page.cursor,
        "snapshot":page.snapshot
    }))
}

fn pending_parent(app: &App, child: &str, input: &Value) -> Result<(String, Value)> {
    let submitted = input["parent"]
        .as_str()
        .ok_or_else(|| problem("A pending native request has no parent operation"))?;
    let parent = submitted
        .split(':')
        .next()
        .filter(|parent| !parent.is_empty())
        .ok_or_else(|| problem("A pending native request has an invalid parent operation"))?;
    if !native_operation_belongs_to(child, parent) {
        return Err(problem("A pending native request belongs to another parent operation"));
    }
    let Some(record) = app.operation(parent)? else {
        // A missing parent must not hide this exact request or the rest of the page.
        // Keep the child as the review identity; do not reconstruct a parent input.
        let mut projection = input.clone();
        projection["command"] = json!("recovery-orphan");
        return Ok((child.to_owned(), projection));
    };
    if record.input["command"] == "daemon-request" {
        return Err(problem("A pending native request has another transport record as its parent"));
    }
    Ok((parent.to_owned(), record.input))
}
pub async fn observe(app: &App, id: &str) -> Result<Value> {
    let Some(mut record) = app.operation(id)? else {
        return Ok(Value::Null);
    };
    let mut effect_observation = None;
    if record.result.is_none() {
        let result = if record.input["command"] == "file-save" {
            let root = app.project(record.input["project"].as_str().unwrap_or(""))?.root;
            let relative = record.input["path"].as_str().unwrap_or("").to_owned();
            let expected = record.input["hash"].as_str().unwrap_or("").to_owned();
            match tokio::task::spawn_blocking(move || -> Result<Option<Value>> {
                let path = files::resolve(&root, &relative)?;
                let (hash, bytes) = files::edit::inspect(&path)?;
                Ok((expected == hash)
                    .then(|| json!({"revision":hash,"bytes":bytes,"recovered":true})))
            }).await {
                Ok(Ok(result)) => result,
                Ok(Err(error)) => {
                    effect_observation = Some(json!({"state":"unverifiable","error":error.0}));
                    None
                },
                Err(error) => {
                    effect_observation = Some(json!({"state":"unverifiable","error":error.to_string()}));
                    None
                },
            }
        } else if ["config", "preferences"]
            .contains(&record.input["command"].as_str().unwrap_or(""))
        {
            recover_configuration(app, &record.input)?
        } else if record.input["command"] == "send" {
            match daemon::recover_send(app, id).await {
                Ok(result) => result,
                Err(error) => return Err(crate::error::uncertain(error.0)),
            }
        } else if ["console", "workbench"]
            .contains(&record.input["command"].as_str().unwrap_or(""))
        {
            recover_console_launch(app, id, &record).await?
        } else if record.input["command"] == "close-console" {
            recover_console_close(app, &record.input).await?
        } else if record.input["command"] == "git" {
            let recovery = crate::git::recover(app, id, record.prepared.as_ref()).await?;
            effect_observation = Some(recovery.observation);
            recovery.result
        } else {
            daemon::receipts::observed(app, id).await?
        };
        if let Some(result) = result {
            let result =
                if record.input["command"] == "session-settings" && result.get("error").is_none() {
                    crate::sessions::save(app, &record.input, &result)?
                } else {
                    result
                };
            record.result = Some(if let Some(owner) = app.try_own_operation(id).await? {
                tokio::task::spawn_blocking(move || -> Result<Value> {
                    if let Some(known) = owner.get()?.and_then(|record| record.result) {
                        return Ok(known);
                    }
                    owner.settle(result.clone())?;
                    Ok(result)
                }).await.map_err(problem)??
            } else {
                // Return the proven observation without queueing behind its active effect owner.
                // That owner, or a later inspection after release, retains the durable result.
                result
            });
        }
    }
    let mut observed = json!(record);
    if let Some(effect) = effect_observation {
        observed["effect"] = effect;
    }
    if record.input["command"] == "file-save" {
        match files::edit::retained(app, id, &record.input) {
            Ok(Some(payload)) => observed["payload"] = payload,
            Ok(None) => {},
            Err(error) => observed["payloadError"] = json!(error.0),
        }
    }
    Ok(observed)
}

async fn recover_console_launch(
    app: &App,
    id: &str,
    record: &crate::state::Operation,
) -> Result<Option<Value>> {
    let Some(prepared) = record.prepared.as_ref() else {
        return Ok(None);
    };
    if prepared["kind"] != "durable-console-launch-v1" {
        return Err(problem("The retained console launch context is not recognized"));
    }
    let console = serde_json::from_value(prepared["console"].clone())?;
    let terminals = std::sync::Arc::clone(&app.terminals);
    let id = id.to_owned();
    tokio::task::spawn_blocking(move || terminals.recover_launch(&id, &console))
        .await.map_err(problem)?
}

async fn recover_console_close(app: &App, input: &Value) -> Result<Option<Value>> {
    let identity = input["id"]
        .as_str()
        .ok_or_else(|| problem("The retained console close identity is missing"))?
        .to_owned();
    let disposition = match input["disposition"].as_str() {
        Some("terminate") => crate::terminal::CloseDisposition::Terminate,
        Some("dismiss") => crate::terminal::CloseDisposition::Dismiss,
        _ => return Err(problem("The retained console close disposition is invalid")),
    };
    let terminals = std::sync::Arc::clone(&app.terminals);
    tokio::task::spawn_blocking(move || terminals.recover_close(&identity, disposition))
        .await.map_err(problem)?
}
fn recover_configuration(app: &App, input: &Value) -> Result<Option<Value>> {
    let current = std::fs::read_to_string(&app.options.config_file)?;
    match input["command"].as_str().unwrap_or("") {
        "config" => {
            let submitted = input["text"].as_str().unwrap_or("");
            if current != submitted {
                return Ok(None);
            }
            Ok(Some(json!(Preferences::parse(submitted)?)))
        }
        "preferences" => {
            let preferences: Preferences = serde_json::from_value(input["preferences"].clone())?;
            let submitted = toml::to_string_pretty(&preferences).map_err(problem)?;
            Preferences::parse(&submitted)?;
            if current != submitted {
                return Ok(None);
            }
            Ok(Some(json!({"preferences":preferences,"config":submitted,"recovered":true})))
        }
        _ => Ok(None),
    }
}
pub async fn acknowledge(app: &App, id: &str) -> Result<Value> {
    observe(app, id).await?;
    let owner = app.own_operation(id).await?;
    let Some(record) = owner.get()? else {
        acknowledge_children(app, id).await?;
        owner.acknowledge_missing_evidence(json!({
            "reviewed":true,
            "acceptance":null,
            "error":"The operation identity survived, but its original input and outcome evidence were missing. The user reviewed the target; no input or acceptance was inferred and nothing was repeated."
        }))?;
        drop(owner);
        return observe(app, id).await;
    };
    if record.input["command"] == "daemon-request" {
        let (parent, _) = pending_parent(app, id, &record.input)?;
        if parent != id {
            return Err(problem("Review the parent operation, not its transport record"));
        }
        // The original transport evidence survives, but its parent does not. Explicit
        // review can settle this exact child without inventing parent acceptance.
    }
    acknowledge_children(app, id).await?;
    if record.result.is_none() {
        owner.settle(
            json!({"reviewed":true,"error":"Outcome manually reviewed by the user. No acceptance was inferred and the operation was not repeated."}),
        )?;
    }
    drop(owner);
    observe(app, id).await
}

async fn acknowledge_children(app: &App, id: &str) -> Result<()> {
    // Preserve native identities and exact request evidence even when acceptance cannot be proved.
    let mut after = None;
    let mut snapshot = None;
    let mut children = Vec::new();
    loop {
        let page = app.pending_operations(after.as_deref(), snapshot.as_deref())?;
        snapshot.get_or_insert_with(|| page.snapshot.clone());
        for (key, _) in page.operations {
            if native_operation_belongs_to(&key, id) {
                children.push(key);
            }
        }
        let Some(cursor) = page.cursor else { break };
        after = Some(cursor);
    }
    for child in children {
        let child = app.own_operation(&child).await?;
        match child.get()? {
            Some(record) if record.result.is_none() => {
                child.settle(json!({"reviewed":true,"acceptance":null,"parent":id}))?;
            }
            None => child.acknowledge_missing_evidence(json!({
                "reviewed":true,
                "acceptance":null,
                "parent":id,
                "error":"Only the transport operation identity survived. Its input and acceptance were not inferred."
            }))?,
            Some(_) => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::Options,
        state::save,
    };
    use std::path::PathBuf;

    fn test_app(root: &std::path::Path) -> App {
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
        .expect("test application")
    }

    #[test]
    fn configuration_recovery_proves_the_exact_persisted_postcondition() {
        let root = tempfile::tempdir().expect("root");
        let app = test_app(root.path());
        let submitted = "theme = \"daylight\"\nfont_family = \"Résumé\"\n";
        save(&app.options.config_file, submitted.as_bytes()).expect("accepted configuration");

        let recovered = recover_configuration(&app, &json!({"command":"config","text":submitted}))
            .expect("configuration recovery")
            .expect("matching postcondition");
        assert_eq!(recovered["theme"], "daylight");

        let preferences = Preferences::default();
        let canonical = toml::to_string_pretty(&preferences).expect("canonical preferences");
        save(&app.options.config_file, canonical.as_bytes()).expect("accepted preferences");
        let recovered = recover_configuration(
            &app,
            &json!({"command":"preferences","preferences":preferences}),
        )
        .expect("preferences recovery")
        .expect("matching postcondition");
        assert_eq!(recovered["config"], canonical);

        assert!(
            recover_configuration(
                &app,
                &json!({"command":"config","text":"theme = \"blueprint\"\n"}),
            )
            .expect("mismatched recovery")
            .is_none()
        );
    }

    #[tokio::test]
    async fn manual_review_settles_children_before_parent_without_deleting_evidence() {
        let root = tempfile::tempdir().expect("root");
        let app = test_app(root.path());
        app.record_operation(
            "original".into(),
            json!({"command":"send"}),
        )
        .unwrap();
        app.record_operation(
            "daemon:original".into(),
            json!({"command":"daemon-request","parent":"original"}),
        )
        .unwrap();
        app.record_operation(
            "daemon:original:workbench:create".into(),
            json!({"command":"daemon-request","parent":"original:workbench:create"}),
        )
        .unwrap();
        app.settle_operation(
            "daemon:original:workbench:create",
            json!({"accepted":true}),
        )
        .unwrap();
        app.record_operation(
            "daemon:original-other".into(),
            json!({"command":"daemon-request","parent":"original-other"}),
        )
        .unwrap();
        app.settle_operation("original", json!({"accepted":true})).unwrap();

        acknowledge(&app, "original").await.expect("manual review");

        assert_eq!(
            app.operation("original").unwrap().unwrap().result.unwrap()["accepted"],
            true
        );
        assert_eq!(
            app.operation("daemon:original").unwrap().unwrap().result.unwrap()["reviewed"],
            true
        );
        assert_eq!(
            app.operation("daemon:original:workbench:create")
                .unwrap()
                .unwrap()
                .result
                .unwrap()["accepted"],
            true
        );
        assert!(app.operation("daemon:original-other").unwrap().unwrap().result.is_none());
    }

    #[tokio::test]
    async fn manual_review_records_index_only_identity_without_inferred_acceptance() {
        let root = tempfile::tempdir().expect("root");
        let app = test_app(root.path());
        app.record_operation(
            "lost-original".into(),
            json!({"command":"send","text":"missing evidence"}),
        )
        .unwrap();
        let namespace = std::fs::read_dir(root.path().join("state/workspace.operations"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let pending = std::fs::read_dir(namespace.join("pending"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        std::fs::remove_file(pending).unwrap();

        let reviewed = acknowledge(&app, "lost-original").await.unwrap();

        assert_eq!(reviewed["input"]["command"], "recovery-unknown");
        assert_eq!(reviewed["result"]["acceptance"], Value::Null);
        assert_eq!(reviewed["result"]["reviewed"], true);
        assert!(reviewed["result"]["error"]
            .as_str()
            .unwrap()
            .contains("no input or acceptance was inferred"));
    }
}
