use peritus_app_client::{Client, RequestIdentity};
use peritus_app_protocol::{
    AppRequestPayload as Request, AppResponsePayload as Response, ControlOperationId,
    ProductInteractionQuery, WellKnownProtocolFeature as Feature, WorkbenchCommand,
    WorkbenchInputId, WorkbenchInputOrder, WorkbenchInputText, WorkbenchIntent, WorkbenchNewInput,
    WorkbenchQueueIntent,
};
use peritus_types::RunId;
use std::time::{Duration, Instant};

type Error = Box<dyn std::error::Error>;
async fn connect(endpoint: &std::ffi::OsStr) -> Result<Client, Error> {
    Ok(Client::connect(
        endpoint,
        None,
        Duration::from_secs(15),
        &[
            Feature::WorkbenchRunBinding,
            Feature::WorkbenchControl,
            Feature::ConversationLibrary,
            Feature::WorkbenchInputs,
        ],
    )
    .await?)
}
async fn request(client: &mut Client, payload: Request) -> Result<Response, Error> {
    Ok(client.request(RequestIdentity::generate()?, payload).await?.payload().clone())
}
#[tokio::main]
async fn main() -> Result<(), Error> {
    let args: Vec<_> = std::env::args_os().collect();
    let s =
        args.get(2).and_then(|v| v.to_str()).ok_or("usage: probe ENDPOINT RUN COUNT [exercise]")?;
    let mut bytes = [0; 16];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)?;
    }
    let run = RunId::new(bytes).map_err(|e| format!("{e:?}"))?;
    let mut c = connect(&args[1]).await?;
    let mut query = None;
    for i in 0..args.get(3).and_then(|v| v.to_str()).unwrap_or("5").parse::<usize>()? {
        let t = Instant::now();
        match request(&mut c, Request::QueryInteractionBinding(ProductInteractionQuery::new(run)))
            .await?
        {
            Response::InteractionBinding(b) => {
                if b.interaction().snapshot().run_id() != run {
                    return Err("wrong run".into());
                }
                if query.is_some_and(|q| q != b.conversation()) {
                    return Err("changed conversation binding".into());
                }
                query = Some(b.conversation());
                println!(
                    "binding {i} elapsed_ms={:.2} received={} activities={}",
                    t.elapsed().as_secs_f64() * 1000.,
                    b.interaction().received(),
                    b.interaction().activities().len()
                );
            }
            v => return Err(format!("unexpected {v:?}").into()),
        }
    }
    if args.get(4).is_none() {
        return Ok(());
    }
    let query = query.ok_or("no binding")?;
    let snapshot = match request(&mut c, Request::QueryWorkbench(query)).await? {
        Response::Workbench(s) => s,
        v => return Err(format!("unexpected {v:?}").into()),
    };
    let original = snapshot.pinned();
    let mut revision = snapshot.revision();
    for i in 0..12 {
        let desired = if i % 2 == 0 { !original } else { original };
        let operation =
            ControlOperationId::new(RequestIdentity::generate()?.request_id.into_bytes())
                .map_err(|e| format!("{e:?}"))?;
        let command = WorkbenchCommand::new(
            operation,
            query,
            revision,
            WorkbenchIntent::PinConversation(desired),
        );
        let t = Instant::now();
        let receipt = match request(&mut c, Request::WorkbenchCommand(command.clone())).await? {
            Response::WorkbenchReceipt(r) => r,
            v => return Err(format!("mutation returned {v:?}").into()),
        };
        if receipt.accepted_revision() != revision + 1 {
            return Err("new append did not advance exactly once".into());
        }
        let again = request(&mut c, Request::WorkbenchCommand(command)).await?;
        if again != Response::WorkbenchReceipt(receipt.clone()) {
            return Err("original operation replay changed receipt".into());
        }
        let stale = WorkbenchCommand::new(
            ControlOperationId::new(RequestIdentity::generate()?.request_id.into_bytes())
                .map_err(|e| format!("{e:?}"))?,
            query,
            revision,
            WorkbenchIntent::PinConversation(!desired),
        );
        if !matches!(request(&mut c, Request::WorkbenchCommand(stale)).await?, Response::Error(_)) {
            return Err("stale revision was accepted".into());
        }
        revision += 1;
        let read = match request(&mut c, Request::QueryWorkbench(query)).await? {
            Response::Workbench(s) => s,
            v => return Err(format!("read returned {v:?}").into()),
        };
        if read.revision() != revision
            || read.pinned() != desired
            || read.title() != snapshot.title()
        {
            return Err("new append, replay, or stale rejection changed unrelated state".into());
        }
        println!(
            "append {i} elapsed_ms={:.2} revision={revision} pinned={desired} idempotent=true stale_rejected=true",
            t.elapsed().as_secs_f64() * 1000.
        );
        if i % 4 == 3 {
            c = connect(&args[1]).await?;
        }
    }
    for i in 0..4 {
        let marker = RequestIdentity::generate()?.request_id;
        let text = format!(
            "Disposable long-history replay input {i} {marker:?}: αβγ 漢字 café\nSecond line\twith a tab."
        );
        let input = WorkbenchNewInput::new(
            WorkbenchInputId::new(RequestIdentity::generate()?.request_id.into_bytes())
                .map_err(|e| format!("{e:?}"))?,
            WorkbenchInputText::new(text.clone())?,
            WorkbenchInputOrder::new(vec![])?,
        )?;
        let command = WorkbenchCommand::new(
            ControlOperationId::new(RequestIdentity::generate()?.request_id.into_bytes())
                .map_err(|e| format!("{e:?}"))?,
            query,
            revision,
            WorkbenchIntent::Queue(WorkbenchQueueIntent::Enqueue(input)),
        );
        match request(&mut c, Request::WorkbenchCommand(command)).await? {
            Response::WorkbenchReceipt(r) if r.accepted_revision() == revision + 1 => revision += 1,
            v => return Err(format!("input returned {v:?}").into()),
        };
        match request(&mut c, Request::QueryInteraction(ProductInteractionQuery::new(run))).await? {
            Response::Interaction(b) if b.activities().iter().any(|a| a.text() == text) => {
                println!("input {i} exact_text_visible=true revision={revision}")
            }
            v => return Err(format!("accepted input missing from projection {v:?}").into()),
        }
    }
    match request(&mut c, Request::QueryWorkbench(query)).await? {
        Response::Workbench(state)
            if state.query() == query
                && state.revision() == revision
                && state.pinned() == original => {}
        value => return Err(format!("final metadata did not preserve state: {value:?}").into()),
    }
    Ok(())
}
