//! Alternating real conversations over negotiated application IPC, including durable mutations.
use peritus_app_client::{Client, RequestIdentity};
use peritus_app_protocol::{
    AppRequestPayload as Request, AppResponsePayload as Response,
    WellKnownProtocolFeature as Feature, *,
};
use std::time::{Duration, Instant};
type Error = Box<dyn std::error::Error>;
fn id() -> Result<[u8; 16], Error> {
    Ok(RequestIdentity::generate()?.request_id.into_bytes())
}
async fn request(c: &mut Client, p: Request) -> Result<Response, Error> {
    Ok(c.request(RequestIdentity::generate()?, p).await?.payload().clone())
}
async fn connect(endpoint: &std::ffi::OsStr) -> Result<Client, Error> {
    Ok(Client::connect(
        endpoint,
        None,
        Duration::from_secs(15),
        &[Feature::WorkbenchControl, Feature::WorkbenchInputs, Feature::ProductDiagnostics],
    )
    .await?)
}
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Error> {
    let a = std::env::args_os().collect::<Vec<_>>();
    let endpoint = &a[1];
    let hex = a[2].to_str().ok_or("workspace")?;
    let mut bytes = [0; 16];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)?;
    }
    let workspace = peritus_types::WorkspaceId::new(bytes).map_err(|e| format!("{e:?}"))?;
    let mut c = connect(endpoint).await?;
    let mut queries = Vec::new();
    let mut revisions = Vec::new();
    let n: usize = a.get(3).and_then(|s| s.to_str()).unwrap_or("6").parse()?;
    for i in 0..n {
        let q = WorkbenchQuery::new(
            ConversationId::new(id()?).map_err(|e| format!("{e:?}"))?,
            workspace,
        );
        let command = WorkbenchCommand::new(
            ControlOperationId::new(id()?).map_err(|e| format!("{e:?}"))?,
            q,
            0,
            WorkbenchIntent::CreateConversation(ConversationTitle::new(format!(
                "Native reuse fixture {i}"
            ))?),
        );
        match request(&mut c, Request::WorkbenchCommand(command)).await? {
            Response::WorkbenchReceipt(r) if r.accepted_revision() == 1 => {}
            v => return Err(format!("create: {v:?}").into()),
        };
        queries.push(q);
        revisions.push(1);
    }
    let mut queries_ms = Vec::new();
    let mut mutation_ms = Vec::new();
    for round in 0..12 {
        for (i, q) in queries.iter().enumerate() {
            let text = format!(
                "round={round} conversation={i}: café 🦀 \n{}",
                "Retained independent evidence. ".repeat(256)
            );
            let input = WorkbenchNewInput::new(
                WorkbenchInputId::new(id()?).map_err(|e| format!("{e:?}"))?,
                WorkbenchInputText::new(text)?,
                WorkbenchInputOrder::new(vec![])?,
            )?;
            let command = WorkbenchCommand::new(
                ControlOperationId::new(id()?).map_err(|e| format!("{e:?}"))?,
                *q,
                revisions[i],
                WorkbenchIntent::Queue(WorkbenchQueueIntent::Enqueue(input)),
            );
            let now = Instant::now();
            let receipt = match request(&mut c, Request::WorkbenchCommand(command.clone())).await? {
                Response::WorkbenchReceipt(r) => r,
                v => return Err(format!("append: {v:?}").into()),
            };
            mutation_ms.push(now.elapsed().as_secs_f64() * 1000.);
            assert_eq!(receipt.accepted_revision(), revisions[i] + 1);
            revisions[i] += 1;
            assert_eq!(
                request(&mut c, Request::WorkbenchCommand(command)).await?,
                Response::WorkbenchReceipt(receipt)
            );
            // A mutation to one conversation must preserve verified reuse and exact state in its neighbors.
            for (j, other) in queries.iter().enumerate() {
                let now = Instant::now();
                match request(&mut c, Request::QueryWorkbench(*other)).await? {
                    Response::Workbench(s) => {
                        assert_eq!(s.revision(), revisions[j]);
                        assert_eq!(s.title().as_str(), format!("Native reuse fixture {j}"));
                    }
                    v => return Err(format!("read: {v:?}").into()),
                };
                queries_ms.push(now.elapsed().as_secs_f64() * 1000.);
            }
        }
        if round % 3 == 2 {
            c = connect(endpoint).await?;
        }
    }
    let report = match request(&mut c, Request::Doctor(DoctorQuery::new(workspace, None))).await? {
        Response::Doctor(r) => r,
        v => return Err(format!("doctor {v:?}").into()),
    };
    let summary = report
        .findings()
        .iter()
        .find(|f| f.check() == "conversation-replay-cache")
        .map(|f| f.observation());
    println!(
        "{}",
        serde_json::json!({"conversations":n,"mutations":mutation_ms.len(),"queries":queries_ms.len(),"query_ms":queries_ms,"mutation_ms":mutation_ms,"cache":summary,"exact_revisions":true,"idempotent_receipts":true})
    );
    Ok(())
}
