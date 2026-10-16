//! Actual public model-protocol archives and event codec, no provider or unit framework.
use peritus_model_protocol::*;
use std::{fs,path::Path,error::Error};
fn report(name:&str,ok:bool,detail:String){println!("{{\"case\":{name:?},\"contract_satisfied\":{ok},\"detail\":{detail:?}}}");}
fn message(text:String)->Result<Message,ProtocolError>{Message::new(Role::User,vec![ContentBlock::Text(BoundedText::new(text,ProtocolLimits::PRODUCTION)?)],ProtocolLimits::PRODUCTION)}
fn event_seed()->EventEnvelope{
 let mut bytes=b"P5EV".to_vec();bytes.extend([0,1,0,1,0,0]);bytes.extend(1u64.to_be_bytes());bytes.extend([0,0]);bytes.extend([0;32]);bytes.extend([1,0,0]);
 let seed=decode_event_envelope(&bytes,ProtocolLimits::PRODUCTION).expect("canonical minimal ResponseStarted seed");assert_eq!(encode_event_envelope(&seed,ProtocolLimits::PRODUCTION).unwrap(),bytes);seed
}
fn event_roundtrip(root:&Path,name:&str,bytes:Vec<u8>,limits:ProtocolLimits)->Result<usize,Box<dyn Error>>{
 fs::write(root.join(format!("{name}.input")),&bytes)?;assert_eq!(fs::read(root.join(format!("{name}.input")))?,bytes);
 let expected=bytes.clone();let event=EventEnvelope::new(1,None,None,event_seed().provider_digest(),ModelEvent::TextDelta{item_id:ItemId::new("fixture-item".into())?,fragment:StreamFragment::new(bytes,limits)?})?;
 let encoded=encode_event_envelope(&event,limits)?;fs::write(root.join(format!("{name}.p5ev")),&encoded)?;
 let decoded=decode_event_envelope(&fs::read(root.join(format!("{name}.p5ev")))?,limits)?;assert_eq!(decoded,event);assert_eq!(encode_event_envelope(&decoded,limits)?,encoded);
 if let ModelEvent::TextDelta{fragment,..}=decoded.event(){assert_eq!(fragment.expose(),expected);}else{panic!("event type changed");}
 Ok(encoded.len())
}
fn ancillary(kind:&str,count:usize)->Result<usize,Box<dyn Error>>{
 let mut reducer=ResponseReducer::new(ProviderName::new("fixture-provider".into())?,ProtocolLimits::PRODUCTION);
 let mut events=vec![ModelEvent::ResponseStarted{response_id:None,model:None}];let mut rates=Vec::new();let mut caches=Vec::new();let mut extensions=Vec::new();
 for n in 0..count{match kind{
  "cache"=>{let value=CacheObservation::new(CacheStatus::Hit,Some(CacheKey::new(format!("fixture-cache-{n}"))?),Some(n as u64),Some(3600));caches.push(value.clone());events.push(ModelEvent::Cache(value));}
  "rate"=>{let value=RateLimitObservation::new(vec![RateLimitWindow::new(RateLimitDimension::Requests,Some(1000),Some(n as u64),Some(ResetTime::AfterMillis(n as u64)))?])?;rates.push(value.clone());events.push(ModelEvent::RateLimit(value));}
  "windows"=>{let value=RateLimitObservation::new((0..count).map(|n|RateLimitWindow::new(RateLimitDimension::Requests,Some(1000),Some(n as u64),None)).collect::<Result<Vec<_>,_>>()?)?;rates.push(value.clone());events.push(ModelEvent::RateLimit(value));break;}
  "extension"=>{let value=ProviderExtension::new(ExtensionName::new("fixture-metadata".into())?,CanonicalJson::parse(&format!("{{\"index\":{n}}}"),JsonBounds::value(ProtocolLimits::PRODUCTION))?);extensions.push(value.clone());events.push(ModelEvent::ProviderEvent(value));}
  _=>panic!("unknown ancillary case")
 }}
 events.push(ModelEvent::Finish(FinishReason::Stop));events.push(ModelEvent::ResponseCompleted);
 for(index,event)in events.into_iter().enumerate(){let envelope=EventEnvelope::new(index as u64+1,None,None,event_seed().provider_digest(),event)?;let encoded=encode_event_envelope(&envelope,ProtocolLimits::PRODUCTION)?;let decoded=decode_event_envelope(&encoded,ProtocolLimits::PRODUCTION)?;assert_eq!(envelope,decoded);reducer.push(decoded)?;}
 assert_eq!(reducer.rate_limits(),rates);assert_eq!(reducer.cache_observations(),caches);assert_eq!(reducer.provider_events(),extensions);assert!(matches!(reducer.finish_eof()?,TerminalOutcome::Succeeded{reason:FinishReason::Stop}));Ok(count)
}
fn main()->Result<(),Box<dyn Error>>{
 let args=std::env::args().collect::<Vec<_>>();let root=Path::new(args.get(1).expect("usage: consumer FRESH_DIR [--final]"));let final_mode=args.iter().any(|v|v=="--final");if root.exists(){return Err("use fresh output directory".into());}fs::create_dir_all(root)?;let mut successes=0;let mut cases=0;
 let messages=(0..4097).map(|n|message(format!("retained-message-{n:05}"))).collect::<Result<Vec<_>,_>>()?;
 let outcome=encode_messages(&messages,ProtocolLimits::PRODUCTION).and_then(|data|{let decoded=decode_messages(&data,ProtocolLimits::PRODUCTION)?;assert_eq!(decoded,messages);fs::write(root.join("messages4097.p5ms"),&data).unwrap();Ok(data.len())});
 report("retain-4097-messages",outcome.is_ok(),format!("{outcome:?}"));successes+=usize::from(outcome.is_ok());cases+=1;
 drop(messages);
 let text=format!("{}\n","é".repeat(8*1024*1024));assert_eq!(text.len(),16*1024*1024+1);fs::write(root.join("text-over16mib.input"),text.as_bytes())?;
 let outcome=message(text.clone()).and_then(|m|{let encoded=encode_messages(&[m.clone()],ProtocolLimits::PRODUCTION)?;let decoded=decode_messages(&encoded,ProtocolLimits::PRODUCTION)?;assert_eq!(decoded,vec![m]);if let ContentBlock::Text(value)=&decoded[0].content()[0]{assert_eq!(value.expose_for_wire(),text);}else{panic!("content type changed");}Ok(encoded.len())});
 report("retain-text-over16mib",outcome.is_ok(),format!("{outcome:?}"));successes+=usize::from(outcome.is_ok());cases+=1;drop(text);
 let json=format!("{{\"text\":\"{}\"}}","x".repeat(16*1024*1024+1));fs::write(root.join("arguments-over16mib.json"),json.as_bytes())?;
 let outcome=CanonicalJson::parse(&json,JsonBounds::value(ProtocolLimits::PRODUCTION));if let Ok(value)=&outcome{assert_eq!(value.to_wire_string(),json);}
 report("retain-tool-json-over16mib",outcome.is_ok(),match &outcome{Ok(v)=>format!("canonical_bytes={}",v.canonical_bytes().len()),Err(e)=>e.to_string()});successes+=usize::from(outcome.is_ok());cases+=1;drop(outcome);drop(json);
 for(name,size,limits)in[("production-event-over8mib",8*1024*1024+1,ProtocolLimits::PRODUCTION),("archive-event-over64mib",65*1024*1024,ProtocolLimits::ARCHIVE)]{
  let outcome=event_roundtrip(root,name,vec![b'a';size],limits);report(name,outcome.is_ok(),match &outcome{Ok(bytes)=>format!("exact_event_roundtrip_bytes={bytes}"),Err(error)=>error.to_string()});successes+=usize::from(outcome.is_ok());cases+=1;
 }
 let flat=format!("{{{}}}",(0..65537).map(|n|format!("\"k{n:05}\":0")).collect::<Vec<_>>().join(","));fs::write(root.join("json65537-members.json"),&flat)?;
 let outcome=CanonicalJson::parse(&flat,JsonBounds::value(ProtocolLimits::PRODUCTION));if let Ok(value)=&outcome{assert_eq!(value.to_wire_string(),flat);}
 report("flattened-json-65537-members",outcome.is_ok(),match &outcome{Ok(value)=>format!("canonical_bytes={}",value.canonical_bytes().len()),Err(error)=>error.to_string()});successes+=usize::from(outcome.is_ok());cases+=1;
 drop(outcome);drop(flat);
 for(kind,count)in[("cache",65),("rate",65),("windows",65),("extension",129)]{let outcome=ancillary(kind,count);report(&format!("retain-{count}-{kind}-metadata"),outcome.is_ok(),match &outcome{Ok(count)=>format!("exact_retained_metadata={count}; terminal=Succeeded"),Err(error)=>error.to_string()});successes+=usize::from(outcome.is_ok());cases+=1;}
 assert!(BoundedText::new("invalid\0text".into(),ProtocolLimits::PRODUCTION).is_err());assert!(CanonicalJson::parse("{\"x\":1,\"x\":2}",JsonBounds::value(ProtocolLimits::PRODUCTION)).is_err());
 report("structural-text-json-guards-retained",true,"NUL text and duplicate JSON keys rejected".into());
 println!("{{\"summary\":\"protocol-boundaries\",\"cases\":{cases},\"observed_success_count\":{successes},\"observed_contract_failures\":{},\"final_mode\":{final_mode}}}",cases-successes);
 if final_mode&&successes!=cases{std::process::exit(1)}if !final_mode&&successes!=0{std::process::exit(2)}Ok(())
}
