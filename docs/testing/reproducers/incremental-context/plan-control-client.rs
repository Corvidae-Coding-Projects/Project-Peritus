//! Operate real daemon controls over negotiated IPC; never substitute execution or providers.
use peritus_app_client::{Client, RequestIdentity};
use peritus_app_protocol::*;
use peritus_types::{ProviderProfileId, RevisionNumber, RunId, WorkspaceId};
use serde_json::{Value, json};
use std::{ffi::OsStr, fs, time::Duration};

type Error = Box<dyn std::error::Error>;
fn operation() -> Result<ControlOperationId, Error> {
    Ok(ControlOperationId::new(RequestIdentity::generate()?.request_id.into_bytes())
        .map_err(|e| format!("{e:?}"))?)
}
async fn request(
    client: &mut Client,
    payload: AppRequestPayload,
) -> Result<AppResponsePayload, Error> {
    let description = format!("{payload:?}");
    let value = client.request(RequestIdentity::generate()?, payload).await?.payload().clone();
    if let AppResponsePayload::Error(error) = &value {
        return Err(format!(
            "{description}: {}: {}",
            error.code().as_str(),
            error.actionable_message()
        )
        .into());
    }
    Ok(value)
}
async fn revision(client: &mut Client, query: WorkbenchQuery) -> Result<u64, Error> {
    match request(client, AppRequestPayload::QueryWorkbench(query)).await? {
        AppResponsePayload::Workbench(value) => Ok(value.revision()),
        other => Err(format!("unexpected query: {other:?}").into()),
    }
}
async fn command(
    client: &mut Client,
    query: WorkbenchQuery,
    revision: u64,
    id: ControlOperationId,
    intent: WorkbenchIntent,
) -> Result<Value, Error> {
    match request(
        client,
        AppRequestPayload::WorkbenchCommand(WorkbenchCommand::new(id, query, revision, intent)),
    )
    .await?
    {
        AppResponsePayload::WorkbenchReceipt(value) => {
            Ok(json!({"revision":value.accepted_revision(),"receipt":format!("{value:?}")}))
        }
        other => Err(format!("unexpected command: {other:?}").into()),
    }
}
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Error> {
    let args: Vec<_> = std::env::args().collect();
    let spec: Value = serde_json::from_slice(&fs::read(args.get(1).ok_or("spec required")?)?)?;
    let action = args.get(2).ok_or("action required")?.as_str();
    if action == "setup" {
        let folder = peritus_workspace::FolderIdentity::observe(std::path::Path::new(
            spec["root"].as_str().ok_or("root required")?,
        ))?;
        let registry =
            peritus_approval::CredentialRegistrySnapshot::new(RevisionNumber::first(), Vec::new())
                .map_err(|e| format!("{e:?}"))?;
        fs::write(
            spec["registry"].as_str().ok_or("registry required")?,
            registry.canonical_bytes().map_err(|e| format!("{e:?}"))?,
        )?;
        println!(
            "{}",
            json!({"folder_identity":folder.digest().as_bytes().iter().map(|v|format!("{v:02x}")).collect::<String>()})
        );
        return Ok(());
    }
    let query = WorkbenchQuery::new(
        ConversationId::new([0x83; 16]).map_err(|e| format!("{e:?}"))?,
        WorkspaceId::new([0x82; 16]).map_err(|e| format!("{e:?}"))?,
    );
    let run = RunId::new([0x84; 16]).map_err(|e| format!("{e:?}"))?;
    let goal = ControlOperationId::new([0x85; 16]).map_err(|e| format!("{e:?}"))?;
    let mut client = Client::connect(
        OsStr::new(spec["endpoint"].as_str().ok_or("endpoint required")?),
        None,
        Duration::from_secs(25),
        &[
            WellKnownProtocolFeature::WorkbenchBrief,
            WellKnownProtocolFeature::ConversationLibrary,
            WellKnownProtocolFeature::WorkbenchControl,
            WellKnownProtocolFeature::WorkbenchInputs,
            WellKnownProtocolFeature::WorkbenchExecution,
            WellKnownProtocolFeature::WorkbenchGoals,
            WellKnownProtocolFeature::WorkbenchPermissions,
            WellKnownProtocolFeature::WorkbenchRunBinding,
        ],
    )
    .await?;
    let result = match action {
        "start" => {
            command(
                &mut client,
                query,
                0,
                operation()?,
                WorkbenchIntent::CreateConversation(ConversationTitle::new(
                    "I5 owned real-control journey".to_owned(),
                )?),
            )
            .await?;
            let model = ProductModelChoice::new("gpt-6.1-sol".to_owned(), false)?;
            let profile = ProviderProfileId::new([0x95; 16]).map_err(|e| format!("{e:?}"))?;
            let settings = WorkbenchExecutionSettings::new(
                run,
                ProductProviderSelection::new(profile, profile, profile),
                if spec["mode"] == "review" {
                    ProductInteractionMode::Review
                } else {
                    ProductInteractionMode::Chat
                },
                ProductRoleModels::new(model.clone(), model.clone(), model),
            );
            let task =
                WorkbenchInputText::new(spec["task"].as_str().ok_or("task required")?.to_owned())?;
            command(
                &mut client,
                query,
                1,
                operation()?,
                WorkbenchIntent::SetBrief {
                    field: WorkbenchBriefField::Objective,
                    text: task.clone(),
                },
            )
            .await?;
            let definition = WorkbenchGoalDefinition::new(
                task,
                vec![WorkbenchGoalCriterionDefinition::new(
                    WorkbenchGoalCriterionKind::RunnerAcceptance,
                    WorkbenchInputText::new("Keep exact facts and current authority".to_owned())?,
                    true,
                )],
            )?;
            command(
                &mut client,
                query,
                2,
                goal,
                WorkbenchIntent::StartGoal { definition, settings },
            )
            .await?
        }
        "inspect" => {
            let interaction = request(
                &mut client,
                AppRequestPayload::QueryInteraction(ProductInteractionQuery::new(run)),
            )
            .await?;
            let goal = request(&mut client, AppRequestPayload::QueryWorkbenchGoal(query)).await?;
            let permissions = match request(
                &mut client,
                AppRequestPayload::QueryWorkbenchPermissions(query),
            )
            .await?
            {
                AppResponsePayload::WorkbenchPermissions(p) => {
                    json!({"revision":p.authority_revision(),
                    "effective":p.entries().iter().map(|entry|(entry.capability().as_str(),entry.effective_allowed())).collect::<std::collections::BTreeMap<_,_>>()})
                }
                other => return Err(format!("unexpected permissions: {other:?}").into()),
            };
            let usage = if let AppResponsePayload::WorkbenchGoal(ref g) = goal {
                json!({"state":format!("{:?}",g.state()),"requests":g.usage().requests(),"tool_calls":g.usage().tool_calls(),
                    "roles":g.usage().roles().iter().map(|r|json!({"role":format!("{:?}",r.role()),"requests":r.requests(),"completed_requests":r.completed_requests(),"tool_calls":r.tool_calls()})).collect::<Vec<_>>()})
            } else {
                json!(null)
            };
            let activities = if let AppResponsePayload::Interaction(ref i) = interaction {
                i.activities().iter().map(|a|json!({"kind":format!("{:?}",a.kind()),"text":a.text(),"detail":a.detail()})).collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            json!({"interaction":format!("{interaction:?}"),"goal":format!("{goal:?}"),"usage":usage,"activities":activities,"permissions":permissions})
        }
        other => {
            let revision = revision(&mut client, query).await?;
            let intent=match other {
                "steer" => WorkbenchIntent::Queue(WorkbenchQueueIntent::Enqueue(WorkbenchNewInput::new(
                    WorkbenchInputId::new(RequestIdentity::generate()?.request_id.into_bytes()).map_err(|e| format!("{e:?}"))?,
                    WorkbenchInputText::new(spec["steering"].as_str().unwrap_or("STOP the old operation sequence. Do not inspect any more files or start any process. Reply exactly I5 STEERING RECEIVED.").to_owned())?,
                    WorkbenchInputOrder::new(Vec::new())?)?)),
                "before-edit" => WorkbenchIntent::PauseGoal{goal,mode:WorkbenchGoalPauseMode::BeforeEdit},
                "pause" => WorkbenchIntent::PauseGoal{goal,mode:WorkbenchGoalPauseMode::AfterOperation},
                "pause-now" => WorkbenchIntent::PauseGoal{goal,mode:WorkbenchGoalPauseMode::Now},
                "clear" => WorkbenchIntent::ClearGoal{goal},
                "resume" => WorkbenchIntent::ResumeGoal{goal},
                "deny-read"|"deny-process"|"allow-process" => {
                    let AppResponsePayload::WorkbenchPermissions(p)=request(&mut client,AppRequestPayload::QueryWorkbenchPermissions(query)).await? else{return Err("missing permissions".into())};
                    let capability=if other=="deny-read" {WorkbenchPermissionCapability::Read} else {WorkbenchPermissionCapability::Process};
                    WorkbenchIntent::SetPermissions(WorkbenchPermissionChange::new(p.authority_revision(),capability,other=="allow-process"))
                },
                _=>return Err("unknown action".into()),
            };
            command(&mut client, query, revision, operation()?, intent).await?
        }
    };
    println!("{result}");
    Ok(())
}
