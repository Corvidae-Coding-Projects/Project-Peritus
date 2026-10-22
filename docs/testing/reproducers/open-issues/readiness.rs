//! Real negotiated IPC admission and recovery, with no substituted provider responses.
use peritus_app_client::{Client, RequestIdentity};
use peritus_app_protocol::*;
use peritus_types::{ProviderProfileId, RunId, WorkspaceId};
use std::time::Duration;
type Error = Box<dyn std::error::Error>;
fn id() -> Result<[u8; 16], Error> {
    Ok(RequestIdentity::generate()?.request_id.into_bytes())
}
fn hex(value: &str) -> Result<[u8; 16], Error> {
    let mut output = [0; 16];
    for (i, byte) in output.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[i * 2..i * 2 + 2], 16)?;
    }
    Ok(output)
}
async fn call(
    client: &mut Client,
    request: AppRequestPayload,
) -> Result<AppResponsePayload, Error> {
    Ok(client.request(RequestIdentity::generate()?, request).await?.payload().clone())
}
async fn command(
    client: &mut Client,
    query: WorkbenchQuery,
    revision: u64,
    intent: WorkbenchIntent,
) -> Result<AppResponsePayload, Error> {
    call(
        client,
        AppRequestPayload::WorkbenchCommand(WorkbenchCommand::new(
            ControlOperationId::new(id()?).map_err(|e| format!("{e:?}"))?,
            query,
            revision,
            intent,
        )),
    )
    .await
}
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Error> {
    let args: Vec<_> = std::env::args().collect();
    let workspace = WorkspaceId::new(hex(&args[2])?).map_err(|e| format!("{e:?}"))?;
    let profile = ProviderProfileId::new(hex(&args[3])?).map_err(|e| format!("{e:?}"))?;
    let missing = ProviderProfileId::new([0xee; 16]).map_err(|e| format!("{e:?}"))?;
    let mut client = Client::connect(
        std::ffi::OsStr::new(&args[1]),
        None,
        Duration::from_secs(20),
        &[WellKnownProtocolFeature::WorkbenchControl, WellKnownProtocolFeature::WorkbenchInputs, WellKnownProtocolFeature::WorkbenchExecution],
    )
    .await?;
    let mut outcomes = Vec::new();
    for scenario in 0..4 {
        let query = WorkbenchQuery::new(
            ConversationId::new(id()?).map_err(|e| format!("{e:?}"))?,
            workspace,
        );
        let created = command(
            &mut client,
            query,
            0,
            WorkbenchIntent::CreateConversation(ConversationTitle::new(format!(
                "Readiness case {scenario}"
            ))?),
        )
        .await?;
        assert!(matches!(created, AppResponsePayload::WorkbenchReceipt(_)), "{created:?}");
        let input = WorkbenchNewInput::new(
            WorkbenchInputId::new(id()?).map_err(|e| format!("{e:?}"))?,
            WorkbenchInputText::new(
                "Retain this pending instruction without starting inference.".into(),
            )?,
            WorkbenchInputOrder::new(vec![])?,
        )?;
        let queued = command(
            &mut client,
            query,
            1,
            WorkbenchIntent::Queue(WorkbenchQueueIntent::Enqueue(input)),
        )
        .await?;
        assert!(matches!(queued, AppResponsePayload::WorkbenchReceipt(_)), "{queued:?}");
        let mut profiles = [profile; 3];
        let model = if scenario < 3 {
            profiles[scenario] = missing;
            "fixture-route-model"
        } else {
            "unadvertised-fixture-model"
        };
        let choice = ProductModelChoice::new(model.to_owned(), false)?;
        let settings = WorkbenchExecutionSettings::new(
            RunId::new(id()?).map_err(|e| format!("{e:?}"))?,
            ProductProviderSelection::new(profiles[0], profiles[1], profiles[2]),
            ProductInteractionMode::Chat,
            ProductRoleModels::new(choice.clone(), choice.clone(), choice),
        );
        let result =
            command(&mut client, query, 2, WorkbenchIntent::StartExecution(settings)).await?;
        let AppResponsePayload::Error(error) = result else {
            return Err(format!("unexpected admission {result:?}").into());
        };
        let message = error.actionable_message();
        outcomes.push(
            serde_json::json!({"scenario":scenario,"code":error.code().as_str(),"message":message}),
        );
        let renamed = command(
            &mut client,
            query,
            2,
            WorkbenchIntent::RenameConversation(ConversationTitle::new(format!(
                "Recovered case {scenario}"
            ))?),
        )
        .await?;
        assert!(matches!(renamed, AppResponsePayload::WorkbenchReceipt(_)), "{renamed:?}");
        let AppResponsePayload::Workbench(snapshot) =
            call(&mut client, AppRequestPayload::QueryWorkbench(query)).await?
        else {
            return Err("missing readback".into());
        };
        assert_eq!(snapshot.revision(), 3);
    }
    println!("{}", serde_json::to_string_pretty(&outcomes)?);
    if args.get(4).is_some_and(|value| value == "--require-fixed") {
        for outcome in &outcomes {
            assert_eq!(outcome["code"], "not-ready");
            assert!(
                outcome["message"].as_str().is_some_and(
                    |text| text.contains("provider") && text.contains("after-recovery")
                )
            );
        }
    }
    Ok(())
}
