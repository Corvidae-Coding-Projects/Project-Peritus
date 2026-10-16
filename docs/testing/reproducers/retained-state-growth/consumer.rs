//! Real public working-state consumer. All input artifacts persist; no mock/provider/test framework.
use peritus_context::{ContextLimits,ContextNodeId,bind_context_content,working::*};
use peritus_codec::sha256;
use peritus_role::HarnessRole;
use peritus_types::{RunId,WorkspaceId};
use serde_json::json;
use std::{fs,path::Path,error::Error};
fn id(n:usize)->ContextNodeId{ContextNodeId::new((n as u128).to_be_bytes()).unwrap()}
fn binding()->WorkingBinding{WorkingBinding::new(RunId::new([11;16]).unwrap(),WorkspaceId::new([12;16]).unwrap(),id(13),HarnessRole::Writer,1)}
fn old_limits()->WorkingLimits{WorkingLimits::new(65535,512,2048,32,32).unwrap()}
fn one_source(root:&Path)->(WorkingState,Vec<u8>){
 let limits=old_limits();let content=b"Persisted host-observed fixture: preserve source locators, entry contents, and dependency identity.".to_vec();
 fs::write(root.join("source.txt"),&content).unwrap();let read=fs::read(root.join("source.txt")).unwrap();assert_eq!(read,content);let digest=sha256(&read);
 let state=WorkingState::new(WorkingEnvironment::new(binding(),sha256(b"candidate"),vec![],limits).unwrap(),limits).unwrap();
 let source=ObservationSource::new(ObservationId::new(1).unwrap(),digest,read.len() as u64,0,read.len()as u64,ObservationKind::ToolOutput).unwrap();
 (ingest_working_observation(&state,binding(),source).unwrap(),read)
}
fn observation_fixture(single:&WorkingState,count:usize,limit:usize)->Vec<u8>{
 // Generate a v1 input from the actual public encoder's legal one-observation snapshot.
 // Expand only its collection. The actual public decoder validates every generated field.
 let bytes=encode_working_state(single).unwrap();let source=single.observation(binding(),ObservationId::new(1).unwrap()).unwrap();
 let digest=source.artifact();let at=bytes.windows(32).position(|b|b==digest.as_bytes()).unwrap()-8;
 assert_eq!(&bytes[at..at+8],&1u64.to_be_bytes());
 let record=&bytes[at..at+65];assert_eq!(record[64],2);
 let mut prefix=bytes[..at].to_vec();let n=prefix.len();assert_eq!(&prefix[n-4..],&1u32.to_be_bytes());
 prefix[n-12..n-4].copy_from_slice(&(count as u64).to_be_bytes());prefix[n-4..].copy_from_slice(&(count as u32).to_be_bytes());
 assert_eq!(&prefix[6..10],&65535u32.to_be_bytes());prefix[6..10].copy_from_slice(&(limit as u32).to_be_bytes());
 let mut output=Vec::with_capacity(prefix.len()+65*count+bytes.len()-at-65);output.extend(prefix);
 for n in 1..=count{output.extend_from_slice(&(n as u64).to_be_bytes());output.extend_from_slice(&record[8..]);}
 output.extend_from_slice(&bytes[at+65..]);output
}
fn record(name:&str,ok:bool,detail:serde_json::Value,final_mode:bool)->bool{
 println!("{}",json!({"case":name,"contract_satisfied":ok,"detail":detail,"mode":if final_mode{"final"}else{"original"}}));ok
}
fn migrate(state:WorkingState)->Result<WorkingState,Box<dyn Error>>{
 #[cfg(feature="growth-api")]
 {
  let next=state.with_limits(WorkingLimits::standard())?;
  assert_eq!(next.binding(),state.binding());assert_eq!(next.revision(),state.revision());
  assert_eq!(next.through_observation(),state.through_observation());
  assert_eq!(next.environment(),state.environment());
  assert_eq!(next.entries(next.binding())?,state.entries(state.binding())?);
  for n in 1..=state.through_observation(){assert_eq!(next.observation(next.binding(),ObservationId::new(n)?)?,state.observation(state.binding(),ObservationId::new(n)?)?);}
  return Ok(next);
 }
 #[cfg(not(feature="growth-api"))]
 Ok(state)
}
#[cfg(feature="growth-api")]
fn paged_roundtrip(state:&WorkingState)->Result<(),Box<dyn Error>>{
 let pages=WorkingStatePages::new(state).collect::<Result<Vec<_>,_>>()?;
 let mut decoder=WorkingStatePageDecoder::new(state.binding(),WorkingLimits::standard());
 for page in &pages{decoder.push(page)?;}
 assert_eq!(decoder.finish()?,*state,"exact paged snapshot roundtrip");
 let mut incomplete=WorkingStatePageDecoder::new(state.binding(),WorkingLimits::standard());
 for page in pages.iter().take(pages.len()-1){incomplete.push(page)?;}
 assert!(incomplete.finish().is_err(),"missing final page accepted");
 let old=state.binding();let foreign=WorkingBinding::new(old.run(),old.workspace(),old.task(),HarnessRole::Reviewer,old.conversation_revision());
 assert!(WorkingStatePageDecoder::new(foreign,WorkingLimits::standard()).push(&pages[0]).is_err(),"foreign role page accepted");
 let mut trailing=pages[0].clone();trailing.push(0);
 let mut poisoned=WorkingStatePageDecoder::new(state.binding(),WorkingLimits::standard());assert!(poisoned.push(&trailing).is_err(),"page trailing bytes accepted");
 for page in &pages[1..]{let _=poisoned.push(page);}assert!(poisoned.finish().is_err(),"ignored trailing-data error later finished valid");
 let mut poisoned=WorkingStatePageDecoder::new(state.binding(),WorkingLimits::standard());for page in &pages{poisoned.push(page)?;}assert!(poisoned.push(&pages[0]).is_err());assert!(poisoned.finish().is_err(),"failed extra page after complete snapshot still finished valid");
 if pages.len()>4{
  let mut duplicate=WorkingStatePageDecoder::new(state.binding(),WorkingLimits::standard());duplicate.push(&pages[0])?;duplicate.push(&pages[1])?;
  assert!(duplicate.push(&pages[1]).is_err(),"duplicate source page accepted");
  let mut gap=WorkingStatePageDecoder::new(state.binding(),WorkingLimits::standard());gap.push(&pages[0])?;
  assert!(gap.push(&pages[2]).is_err(),"missing source page accepted");
  // PWMP v2 source page: header(6),stage(1),offset(8),count(4),source ID(8).
  let mut tampered=pages[1].clone();assert_eq!(tampered[6],1);tampered[19]^=1;
  let mut decoder=WorkingStatePageDecoder::new(state.binding(),WorkingLimits::standard());decoder.push(&pages[0])?;
  assert!(decoder.push(&tampered).is_err(),"noncontiguous tampered source identity accepted");
  for page in &pages[1..]{let _=decoder.push(page);}assert!(decoder.finish().is_err(),"ignored source tamper later finished valid");
 }
 println!("{}",json!({"case":"paged-snapshot-invariants","sources":state.through_observation(),"entries":state.entries(state.binding())?.len(),"pages":pages.len(),"largest_page_bytes":pages.iter().map(Vec::len).max().unwrap(),"exact_roundtrip":true,"missing_final_rejected":true,"foreign_role_rejected":true,"trailing_data_rejected":true,"failed_decoder_poisoned":true,"multi_source_page_tamper_gap_duplicate_checked":pages.len()>4}));
 Ok(())
}
fn roundtrip(state:&WorkingState)->Result<usize,Box<dyn Error>>{
 #[cfg(feature="growth-api")]paged_roundtrip(state)?;
 let b=encode_working_state(state)?;let restored=decode_working_state(&b,state.binding(),WorkingLimits::standard())?;assert_eq!(&restored,state);assert_eq!(encode_working_state(&restored)?,b);Ok(b.len())}
fn entry(state:&WorkingState,n:usize,content:Vec<u8>,supports:Vec<ObservationId>)->Result<WorkingEntry,Box<dyn Error>>{
 let limits=state.limits();let caps=ContextLimits::new(10000,content.len().max(1),10000,5).unwrap();
 Ok(WorkingEntry::new(id(n),WorkingEntryKind::Plan,bind_context_content(content.clone(),sha256(&content),caps).unwrap(),WorkingLinks::new(supports,vec![],vec![],limits)?,WorkingValidity::new(None,None,vec![],limits)?,limits)?)
}
fn main()->Result<(),Box<dyn Error>>{
 let args=std::env::args().collect::<Vec<_>>();let root=Path::new(args.get(1).expect("usage: consumer FRESH_DIR [--final] [--reuse-original FIXTURE_DIR]"));let final_mode=args.iter().any(|a|a=="--final");
 if root.exists(){return Err("Use a fresh output directory".into());}fs::create_dir_all(root)?;
 let(mut successes,mut results)=(0usize,0usize);let(single,source)=one_source(root);let single=migrate(single)?;
 let reuse=args.iter().position(|a|a=="--reuse-original").map(|n|Path::new(&args[n+1]));
 let snapshot=if let Some(old)=reuse{fs::read(old.join("original-65535.pwms"))?}else{observation_fixture(&single,65535,65535)};
 fs::write(root.join("original-65535.pwms"),&snapshot)?;
 let full=migrate(decode_working_state(&snapshot,binding(),WorkingLimits::standard())?)?;assert_eq!(full.through_observation(),65535);
 assert_eq!(full.observation(binding(),ObservationId::new(65535).unwrap())?.artifact(),sha256(&source));
 println!("{}",json!({"case":"legal-original-65535-snapshot","bytes":snapshot.len(),"sha256":format!("{:?}",sha256(&snapshot)),"decoded_observations":full.through_observation(),"verified_last_source":true}));
 let next=ObservationSource::new(ObservationId::new(65536)?,sha256(&source),source.len()as u64,0,source.len()as u64,ObservationKind::ToolOutput)?;
 let outcome=ingest_working_observation(&full,binding(),next);
 let detail=match &outcome{Ok(s)=>{assert_eq!(s.through_observation(),65536);assert_eq!(s.observation(binding(),ObservationId::new(1)?)?.artifact(),sha256(&source));assert_eq!(s.observation(binding(),ObservationId::new(65536)?)?,next);json!({"through":s.through_observation(),"roundtrip_bytes":roundtrip(s)?})},Err(e)=>json!({"error":format!("{e:?}")})};
 successes+=usize::from(record("ingest-observation-65536-from-original-snapshot",outcome.is_ok(),detail,final_mode));results+=1;
 // Deliberately larger valid-format source index isolates the original 8MiB decoder boundary.
 let large=if let Some(old)=reuse{fs::read(old.join("original-over-8mib.pwms"))?}else{observation_fixture(&single,130000,130000)};
 assert!(large.len()>8*1024*1024);fs::write(root.join("original-over-8mib.pwms"),&large)?;
 let outcome=decode_working_state(&large,binding(),WorkingLimits::standard());
 let detail=match &outcome{Ok(s)=>{assert_eq!(s.through_observation(),130000);assert_eq!(s.observation(binding(),ObservationId::new(130000)?)?.artifact(),sha256(&source));json!({"bytes":large.len(),"through":s.through_observation(),"roundtrip_bytes":roundtrip(s)?})},Err(e)=>json!({"bytes":large.len(),"error":format!("{e:?}")})};
 successes+=usize::from(record("decode-persist-more-than-8mib",outcome.is_ok(),detail,final_mode));results+=1;
 let legacy_single=encode_working_state(&single)?;let mut state=migrate(decode_working_state(&legacy_single,binding(),WorkingLimits::standard())?)?;
 if let Some(old)=reuse{state=migrate(decode_working_state(&fs::read(old.join("original-512-entries.pwms"))?,binding(),WorkingLimits::standard())?)?;assert_eq!(state.entries(binding())?.len(),512);}
 else{for n in 1..=512{let e=entry(&state,n,format!("required-existing-{n}").into_bytes(),vec![ObservationId::new(1)?])?;state=apply_working_delta(&state,&WorkingDelta::new(binding(),state.revision(),vec![e],state.limits())?)?;}}
 fs::write(root.join("original-512-entries.pwms"),encode_working_state(&state)?)?;
 let outcome=entry(&state,513,b"required-new-513".to_vec(),vec![ObservationId::new(1)?]).and_then(|e|Ok(apply_working_delta(&state,&WorkingDelta::new(binding(),state.revision(),vec![e],state.limits())?)?));
 let detail=match &outcome{Ok(s)=>{assert_eq!(s.entries(binding())?.len(),513);for n in 1..=513{assert_eq!(s.entry(binding(),id(n))?.id(),id(n));}json!({"entries":513,"roundtrip_bytes":roundtrip(s)?})},Err(e)=>json!({"error":e.to_string()})};successes+=usize::from(record("add-entry-513-retaining-first-512",outcome.is_ok(),detail,final_mode));results+=1;
 let large_text="🦀".repeat(2304).into_bytes();fs::write(root.join("large-entry-source.txt"),&large_text)?;
 let outcome=entry(&single,1000,large_text.clone(),vec![ObservationId::new(1)?]).and_then(|e|Ok(apply_working_delta(&single,&WorkingDelta::new(binding(),single.revision(),vec![e],single.limits())?)?));
 let detail=match &outcome{Ok(s)=>{assert_eq!(s.entry(binding(),id(1000))?.content().bytes(),large_text);json!({"entry_bytes":large_text.len(),"roundtrip_bytes":roundtrip(s)?})},Err(e)=>json!({"entry_bytes":large_text.len(),"error":e.to_string()})};successes+=usize::from(record("unicode-entry-above-2048-bytes",outcome.is_ok(),detail,final_mode));results+=1;
 let supports=(1..=33).map(|n|ObservationId::new(n).unwrap()).collect();let outcome=entry(&full,1001,b"required-links-33".to_vec(),supports).and_then(|e|Ok(apply_working_delta(&full,&WorkingDelta::new(binding(),full.revision(),vec![e],full.limits())?)?));
 let detail=match &outcome{Ok(s)=>{assert_eq!(s.entry(binding(),id(1001))?.links().supports().len(),33);json!({"source_links":33,"roundtrip_bytes":roundtrip(s)?})},Err(e)=>json!({"error":e.to_string()})};successes+=usize::from(record("more-than-32-source-links",outcome.is_ok(),detail,final_mode));results+=1;
 fs::create_dir(root.join("file-dependencies"))?;let mut files=Vec::new();
 for n in 1..=513{let data=format!("verified-file-{n}").into_bytes();let path=root.join("file-dependencies").join(format!("file-{n:04}.txt"));fs::write(&path,&data)?;let read=fs::read(&path)?;assert_eq!(read,data);files.push(WorkingFileDigest::new(id(n),sha256(&read)));}
 let outcome=WorkingEnvironment::new(binding(),sha256(b"candidate"),files.clone(),single.limits());let detail=match &outcome{Ok(env)=>{assert_eq!(env.files(),files);json!({"file_dependencies":env.files().len()})},Err(e)=>json!({"error":format!("{e:?}")})};successes+=usize::from(record("more-than-512-environment-file-dependencies",outcome.is_ok(),detail,final_mode));results+=1;
 let batch=(2000..2033).map(|n|entry(&single,n,format!("batch-entry-{n}").into_bytes(),vec![ObservationId::new(1).unwrap()])).collect::<Result<Vec<_>,_>>()?;
 let outcome=WorkingDelta::new(binding(),single.revision(),batch,single.limits()).and_then(|d|apply_working_delta(&single,&d));let detail=match &outcome{Ok(s)=>{assert_eq!(s.entries(binding())?.len(),33);json!({"batch_entries":33,"roundtrip_bytes":roundtrip(s)?})},Err(e)=>json!({"error":format!("{e:?}")})};successes+=usize::from(record("more-than-32-update-operations",outcome.is_ok(),detail,final_mode));results+=1;
 let expected_failures=if final_mode{0}else{7};let observed_failures=results-successes;
 println!("{}",json!({"summary":"working-state-boundaries","cases":results,"observed_success_count":successes,"observed_contract_failures":observed_failures,"expected_original_failures":expected_failures,"final_mode":final_mode}));if (final_mode&&observed_failures!=0)||(!final_mode&&observed_failures!=7){std::process::exit(1)}Ok(())
}
