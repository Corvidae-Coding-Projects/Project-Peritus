//! Actual run selection/readback through negotiated application IPC.
use peritus_app_client::{Client, RequestIdentity};
use peritus_app_protocol::*;
use peritus_types::{ProviderProfileId, RunId};
use std::time::Duration;
type Error = Box<dyn std::error::Error>;
fn id(value: &str) -> Result<[u8; 16], Error> {
    let mut bytes = [0; 16];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[i * 2..i * 2 + 2], 16)?;
    }
    Ok(bytes)
}
fn hex(value: &[u8]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}
async fn call(
    client: &mut Client,
    payload: AppRequestPayload,
) -> Result<AppResponsePayload, Error> {
    Ok(client.request(RequestIdentity::generate()?, payload).await?.payload().clone())
}
async fn inspect(client: &mut Client, run: RunId) -> Result<ProductInteractionSnapshot, Error> {
    match call(client, AppRequestPayload::QueryInteraction(ProductInteractionQuery::new(run)))
        .await?
    {
        AppResponsePayload::Interaction(value) => Ok(value),
        value => Err(format!("unexpected {value:?}").into()),
    }
}
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Error> {
    let args: Vec<_> = std::env::args().collect();
    let run = RunId::new(id(&args[2])?).map_err(|e| format!("{e:?}"))?;
    let mut client = Client::connect(
        std::ffi::OsStr::new(&args[1]),
        None,
        Duration::from_secs(15),
        &[WellKnownProtocolFeature::ProviderSelection],
    )
    .await?;
    let mut snapshot = inspect(&mut client, run).await?;
    if args.get(3).is_some_and(|value| value == "reject") {
        let missing = ProviderProfileId::new([0xee; 16]).map_err(|e| format!("{e:?}"))?;
        let update = ProductModelUpdate::new(run, ProductRoleModels::default())
            .with_providers(ProductProviderSelection::new(missing, missing, missing));
        let result = call(&mut client, AppRequestPayload::UpdateModels(update)).await?;
        assert!(
            matches!(result,AppResponsePayload::Error(ref error) if error.code()==AppErrorCode::NotReady),
            "{result:?}"
        );
        let after = inspect(&mut client, run).await?;
        assert_eq!(snapshot, after, "Rejected provider changed the retained run");
    }
    if args.get(3).is_some_and(|value| value == "legacy-model") {
        let result = call(
            &mut client,
            AppRequestPayload::UpdateModels(ProductModelUpdate::new(
                run,
                ProductRoleModels::default(),
            )),
        )
        .await?;
        let AppResponsePayload::Interaction(after) = result else {
            return Err(format!("legacy model update {result:?}").into());
        };
        assert_eq!(snapshot.snapshot().providers(), after.snapshot().providers());
        snapshot = after;
    }
    if args.get(3).is_some_and(|value| value == "efforts") {
        let profiles = snapshot.snapshot().providers();
        for role in 0..3 {
            let mut choices = [
                snapshot.models().writer().clone(),
                snapshot.models().reviewer().clone(),
                snapshot.models().fixer().clone(),
            ];
            choices[role] = ProductModelChoice::default().with_effort(
                [ProductModelEffort::Low, ProductModelEffort::Medium, ProductModelEffort::High]
                    [role],
            );
            let models =
                ProductRoleModels::new(choices[0].clone(), choices[1].clone(), choices[2].clone());
            let update = ProductModelUpdate::new(run, models.clone()).with_providers(profiles);
            let result = call(&mut client, AppRequestPayload::UpdateModels(update)).await?;
            let AppResponsePayload::Interaction(after) = result else {
                return Err(format!("effort-bearing provider update {result:?}").into());
            };
            assert_eq!(after.models(), &models);
            assert_eq!(after.snapshot().providers(), profiles);
            snapshot = after;
        }
    }
    let providers = snapshot.snapshot().providers();
    let activities=snapshot.activities().iter().map(|activity|serde_json::json!({"kind":format!("{:?}",activity.kind()),"text":activity.text(),"detail":activity.detail()})).collect::<Vec<_>>();
    println!(
        "{}",
        serde_json::json!({"run":hex(run.as_bytes()),"phase":format!("{:?}",snapshot.snapshot().phase()),"providers":[hex(providers.writer().as_bytes()),hex(providers.reviewer().as_bytes()),hex(providers.fixer().as_bytes())],"models":[snapshot.models().writer().id(),snapshot.models().reviewer().id(),snapshot.models().fixer().id()],"activities":activities})
    );
    Ok(())
}
