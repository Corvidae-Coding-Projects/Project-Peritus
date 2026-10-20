//! Reads one real conversation after external SQLite commits or deliberate corruption.
use peritus_app_client::{Client, RequestIdentity};
use peritus_app_protocol::*;
use std::time::Duration;
fn parse(s: &str) -> [u8; 16] {
    let mut b = [0; 16];
    for (i, v) in b.iter_mut().enumerate() {
        *v = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap();
    }
    b
}
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a = std::env::args().collect::<Vec<_>>();
    let mut c = Client::connect(
        a[1].as_ref(),
        None,
        Duration::from_secs(10),
        &[WellKnownProtocolFeature::WorkbenchControl, WellKnownProtocolFeature::ProductDiagnostics],
    )
    .await?;
    let workspace = peritus_types::WorkspaceId::new(parse(&a[2])).map_err(|e| format!("{e:?}"))?;
    let query = WorkbenchQuery::new(
        ConversationId::new(parse(&a[3])).map_err(|e| format!("{e:?}"))?,
        workspace,
    );
    let response =
        c.request(RequestIdentity::generate()?, AppRequestPayload::QueryWorkbench(query)).await?;
    if a[4] == "corrupt" {
        assert!(
            matches!(response.payload(), AppResponsePayload::Error(_)),
            "corrupt durable state was served from stale cache"
        );
    } else {
        assert!(matches!(response.payload(), AppResponsePayload::Workbench(_)), "{response:?}");
    }
    let response = c
        .request(
            RequestIdentity::generate()?,
            AppRequestPayload::Doctor(DoctorQuery::new(workspace, None)),
        )
        .await?;
    if let AppResponsePayload::Doctor(report) = response.payload() {
        let finding = report
            .findings()
            .iter()
            .find(|f| f.check() == "conversation-replay-cache")
            .ok_or("missing cache diagnostics")?;
        println!(
            "{}",
            serde_json::json!({"case":a[4],"cache":finding.observation(),"passed":true})
        );
    } else {
        return Err(format!("{response:?}").into());
    }
    Ok(())
}
