use peritus_app_client::{Client, RequestIdentity};
use peritus_app_protocol::*;
use std::{
    io::Write,
    time::{Duration, Instant},
};
fn id(s: &str) -> [u8; 16] {
    let mut b = [0; 16];
    for (i, v) in b.iter_mut().enumerate() {
        *v = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap();
    }
    b
}
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a = std::env::args().collect::<Vec<_>>();
    let workspace = peritus_types::WorkspaceId::new(id(&a[2])).map_err(|e| format!("{e:?}"))?;
    let ids: Vec<String> = serde_json::from_slice(&std::fs::read(&a[3])?)?;
    let queries = ids
        .iter()
        .map(|s| WorkbenchQuery::new(ConversationId::new(id(s)).unwrap(), workspace))
        .collect::<Vec<_>>();
    let mut c = Client::connect(
        a[1].as_ref(),
        None,
        Duration::from_secs(15),
        &[WellKnownProtocolFeature::WorkbenchControl, WellKnownProtocolFeature::ProductDiagnostics],
    )
    .await?;
    for round in 0..8 {
        let mut latencies = vec![];
        let start = Instant::now();
        for i in 0..1000 {
            let t = Instant::now();
            let response = c
                .request(
                    RequestIdentity::generate()?,
                    AppRequestPayload::QueryWorkbench(queries[i % queries.len()]),
                )
                .await?;
            assert!(matches!(response.payload(), AppResponsePayload::Workbench(_)));
            latencies.push(t.elapsed().as_secs_f64() * 1e6);
        }
        latencies.sort_by(f64::total_cmp);
        let response = c
            .request(
                RequestIdentity::generate()?,
                AppRequestPayload::Doctor(DoctorQuery::new(workspace, None)),
            )
            .await?;
        let summary = if let AppResponsePayload::Doctor(report) = response.payload() {
            report
                .findings()
                .iter()
                .find(|f| f.check() == "conversation-replay-cache")
                .unwrap()
                .observation()
        } else {
            panic!("no diagnostics")
        };
        let status = std::fs::read_to_string(format!("/proc/{}/status", a[4]))?;
        let memory = status
            .lines()
            .filter(|line| line.starts_with("VmRSS:") || line.starts_with("VmHWM:"))
            .collect::<Vec<_>>();
        println!(
            "{}",
            serde_json::json!({"round":round,"queries":1000,"seconds":start.elapsed().as_secs_f64(),"median_us":latencies[500],"p95_us":latencies[950],"memory":memory,"cache":summary})
        );
        std::io::stdout().flush()?;
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    Ok(())
}
