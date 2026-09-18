//! Saved launcher metadata is not proof that the running daemon admits a workspace.
use super::{
    App, AppRequestPayload, AppResponsePayload, ProductInteractionRequest, Project,
    ProviderProfileId, Result, Value, WorkspaceId, bytes, facts, json, problem, request, status,
};
use peritus_app_protocol::{DoctorQuery, DoctorStatus};

pub async fn ready_facts(app: &App, project: &Project) -> Result<Value> {
    let mut value = facts(app, project)?;
    value["ready"] = json!(false);
    value["reason"] = json!("Set up this project and a provider in the console.");
    value["rememberedWorkspace"] = value["workspace"].clone();
    let Some(id) = value["workspace"]["id"].as_str() else { return Ok(value) };
    let workspace = WorkspaceId::new(bytes(id)?).map_err(|e| problem(format!("{e:?}")))?;
    let provider = value["providers"][0]["id"]
        .as_str()
        .map(|id| ProviderProfileId::new(bytes(id)?).map_err(|e| problem(format!("{e:?}"))))
        .transpose()?;
    match request(app, AppRequestPayload::Doctor(DoctorQuery::new(workspace, provider))).await {
        Ok(AppResponsePayload::Doctor(report)) => {
            let admitted = report
                .findings()
                .iter()
                .any(|f| f.check() == "workspace-policy" && f.status() == DoctorStatus::Healthy);
            let ready = admitted
                && report
                    .findings()
                    .iter()
                    .any(|f| f.check() == "provider-route" && f.status() == DoctorStatus::Healthy);
            value["ready"] = json!(ready);
            value["reason"] = json!(if ready {
                "Workspace and provider registered in the running daemon. Provider authentication is checked when used.".to_owned()
            } else {
                report
                    .findings()
                    .iter()
                    .filter(|f| f.status() == DoctorStatus::Blocked)
                    .map(|f| format!("{} {}", f.observation(), f.action()))
                    .collect::<Vec<_>>()
                    .join(" ")
            });
            value["diagnostics"]=json!(report.findings().iter().map(|f|json!({"check":f.check(),"status":f.status().label(),"observation":f.observation(),"action":f.action()})).collect::<Vec<_>>());
            if !admitted {
                value["workspace"] = Value::Null;
            }
        }
        Ok(_) => {
            value["workspace"] = Value::Null;
            value["reason"] = json!("The daemon did not return workspace diagnostics.");
        }
        Err(error) => {
            value["workspace"] = Value::Null;
            value["reason"] =
                json!(format!("Workspace admission could not be verified: {}", error.0));
        }
    }
    Ok(value)
}
pub async fn admit(app: &App, prepared: &ProductInteractionRequest) -> Result<()> {
    let status = status(app).await?;
    if status["ready"] != true {
        return Err(problem(format!(
            "Daemon is {}. New messages are disabled until it is ready for mutations.",
            status["readiness"].as_str().unwrap_or("unavailable")
        )));
    }
    let selected = prepared.request().providers();
    for provider in
        std::collections::BTreeSet::from([selected.writer(), selected.reviewer(), selected.fixer()])
    {
        match request(
            app,
            AppRequestPayload::Doctor(DoctorQuery::new(
                prepared.request().workspace_id(),
                Some(provider),
            )),
        )
        .await?
        {
            AppResponsePayload::Doctor(report) => {
                if let Some(finding) =
                    report.findings().iter().find(|f| f.status() == DoctorStatus::Blocked)
                {
                    return Err(problem(format!("{} {}", finding.observation(), finding.action())));
                }
            }
            _ => return Err(problem("Could not verify workspace/provider admission")),
        }
    }
    Ok(())
}
