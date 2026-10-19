//! Real public IPC consumer of automatic checkpoint inspection and selected-file rewind.
use peritus_app_client::{Client,RequestIdentity};
use peritus_app_protocol::{AppRequestPayload as Request,AppResponsePayload as Response,WellKnownProtocolFeature as Feature,ProductInteractionQuery,ControlOperationId,WorkbenchCheckpointVersion,WorkbenchCommand,WorkbenchIntent,WorkbenchRewindRequest};
use peritus_types::RunId;
use serde_json::{Value,json};
use std::{error::Error,fs,time::Duration};
type Result<T>=std::result::Result<T,Box<dyn Error>>;
fn bytes(text:&str)->Result<[u8;16]>{if text.len()!=32{return Err("expected 16-byte hex id".into())};let mut b=[0;16];for(i,v)in b.iter_mut().enumerate(){*v=u8::from_str_radix(&text[i*2..i*2+2],16)?;}Ok(b)}
fn version(v:WorkbenchCheckpointVersion)->Value{match v {WorkbenchCheckpointVersion::Absent=>json!({"kind":"absent"}),WorkbenchCheckpointVersion::Present{digest,bytes,mode}=>json!({"kind":"present","digest":digest.as_bytes().iter().map(|b|format!("{b:02x}")).collect::<String>(),"bytes":bytes,"mode":format!("{mode:?}")})}}
async fn request(c:&mut Client,r:Request)->Result<Response>{Ok(c.request(RequestIdentity::generate()?,r).await?.payload().clone())}
async fn run()->Result<()> {
 let a=std::env::args_os().collect::<Vec<_>>();let run=RunId::new(bytes(a[2].to_str().ok_or("run id UTF8")?)?).map_err(|e|format!("{e:?}"))?;
 let ids:Vec<String>=serde_json::from_slice(&fs::read(&a[3])?)?;let restore=a.get(4).is_some_and(|s|s=="restore-first");
 let mut client=Client::connect(&a[1],None,Duration::from_secs(30),&[Feature::WorkbenchRunBinding,Feature::WorkbenchControl,Feature::WorkbenchCheckpoints]).await?;
 let query=match request(&mut client,Request::QueryInteractionBinding(ProductInteractionQuery::new(run))).await? {Response::InteractionBinding(binding)=>binding.conversation(),v=>return Err(format!("binding: {v:?}").into())};
 let revision=match request(&mut client,Request::QueryWorkbench(query)).await? {Response::Workbench(snapshot)=>snapshot.revision(),v=>return Err(format!("snapshot: {v:?}").into())};
 let mut rows=Vec::new();let mut first=None;
 for id in ids {let id=ControlOperationId::new(bytes(&id)?).map_err(|e|format!("{e:?}"))?;let selected=WorkbenchRewindRequest::new(query,revision,id)?;
  let receipt=match request(&mut client,Request::InspectWorkbenchCheckpoint(selected)).await?{Response::WorkbenchCheckpoint(value)=>value,v=>return Err(format!("checkpoint: {v:?}").into())};
  rows.push(json!({"checkpoint":id.as_bytes().iter().map(|b|format!("{b:02x}")).collect::<String>(),"paths":receipt.paths().iter().map(|path|json!({"path":path.path(),"before":version(path.checkpoint()),"after":path.expected_current().map(version)})).collect::<Vec<_>>()}));
  if first.is_none(){first=Some(selected)}
 }
 let restored=if restore {let selected=first.ok_or("no checkpoint to restore")?;let preview=match request(&mut client,Request::PreviewWorkbenchRewind(selected)).await?{Response::WorkbenchRewindPreview(value)=>value,v=>return Err(format!("preview: {v:?}").into())};
  let command=WorkbenchCommand::new(ControlOperationId::new(RequestIdentity::generate()?.request_id.into_bytes()).map_err(|e|format!("{e:?}"))?,query,revision,WorkbenchIntent::ApplyRewind(preview));
  match request(&mut client,Request::WorkbenchCommand(command)).await? {Response::WorkbenchRestore(receipt)=>json!({"status":format!("{:?}",receipt.status()),"restored":receipt.restored(),"conflicts":receipt.conflicts()}),v=>return Err(format!("restore: {v:?}").into())}
 } else {Value::Null};println!("{}",json!({"checkpoints":rows,"restore":restored}));Ok(())
}
fn main()->Result<()>{tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build()?.block_on(run())}
