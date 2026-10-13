use peritus_app_client::{Client,RequestIdentity};
use peritus_app_protocol::{AppRequestPayload as Q,AppResponsePayload as A,ProductInteractionQuery,WorkbenchQueueQuery,WellKnownProtocolFeature as F};
use peritus_types::RunId;
#[tokio::main]async fn main()->Result<(),Box<dyn std::error::Error>>{
 let a:Vec<_>=std::env::args().collect();let mut b=[0;16];for(i,v)in b.iter_mut().enumerate(){*v=u8::from_str_radix(&a[2][2*i..2*i+2],16)?;}
 let mut c=Client::connect(a[1].as_ref(),None,std::time::Duration::from_secs(20),&[F::WorkbenchRunBinding,F::WorkbenchInputs]).await?;
 let r=c.request(RequestIdentity::generate()?,Q::QueryInteractionBinding(ProductInteractionQuery::new(RunId::new(b).map_err(|e|format!("{e:?}"))?))).await?;
 let A::InteractionBinding(binding)=r.payload() else{return Err(format!("unexpected {:?}",r.payload()).into())};
 let q=WorkbenchQueueQuery::new(binding.conversation(),0,0,true)?;
 let r=c.request(RequestIdentity::generate()?,Q::QueryWorkbenchQueue(q)).await?;
 let A::WorkbenchQueue(page)=r.payload() else{return Err(format!("unexpected {:?}",r.payload()).into())};
 println!("total={} revision={}",page.total(),page.query().revision());for row in page.rows(){println!("INPUT {:?} dependencies={:?}\n{}",row.selected(),row.dependencies(),row.text().as_str());}Ok(())}
