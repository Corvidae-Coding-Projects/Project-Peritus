use std::{error::Error, fs, path::{Path,PathBuf}, process::Command, time::Instant};
use peritus_eval::*;
use peritus_types::*;
use peritus_harness::domain::*;
use peritus_model_protocol::{ProviderProfile,ProviderName,ModelName,WireDialect,CapabilityMatrix,CapabilityProvenance,ModelLimits,OutputLimitEnforcement,StateMode,ResumeKind,CancellationKind};
use peritus_artifact_store::{ArtifactStore,StoreConfig,ArtifactDigest as StoredDigest,WriteRequest,EncryptionMetadata,MediaType as StoredMedia};
use peritus_journal::{SqliteJournal,SqliteJournalOptions,StoreId};
use peritus_scheduler::{ResourceVector,ResourceEntry,ResourceKind,ResourceQuantity};
use serde::{Serialize,Deserialize};
type AnyResult<T> = Result<T,Box<dyn Error>>;
fn digest(bytes: &[u8])->Sha256Digest {peritus_codec::sha256(bytes)}
fn eid(n:u8)->EventId {EventId::new([n;16]).unwrap()}
fn save(store:&ArtifactStore,bytes:&[u8])->AnyResult<StoredDigest>{
 let d=StoredDigest::from_sha256(digest(bytes));
 let mut w=store.begin_write(WriteRequest::new(d,bytes.len() as u64,bytes.len().max(1) as u64,StoredMedia::new("application/octet-stream")?,EncryptionMetadata::unencrypted(),eid(201)))?;
 w.write_chunk(bytes)?; Ok(w.finalize()?.digest())
}
fn read(store:&ArtifactStore,d:StoredDigest)->AnyResult<Vec<u8>>{
 let mut r=store.open_read(d)?;let mut bytes=Vec::new();
 while let Some(chunk)=r.read_chunk(65536)?{bytes.extend_from_slice(chunk.bytes());} Ok(bytes)
}
fn require(ok:bool,detail:&str)->AnyResult<()> {if !ok {return Err(detail.to_owned().into())} Ok(())}
fn revision(executable:&Path,previous:Option<&HarnessRevision>)->AnyResult<HarnessRevision>{
 let bytes=fs::read(executable)?;
 // A genuine checked E1 graph binds the executable bytes. It remains inert;
 // no E1 materialization receipt or production harness authority is manufactured.
 let description=format!("CLI executable sha256 {:?}\n",digest(&bytes));
 let payload=description.as_bytes().to_vec();
 let kind=ComponentKind::RolePrompt; let schema=SchemaVersion::new(1)?;
 let declaration=ComponentDeclaration::new(
  ComponentIdentity::new(ComponentId::new("cli-contract")?,kind,schema),
  ComponentLocation::new(SourcePath::new(".peritus-harness/components/cli-contract.txt")?,TargetPath::new("cli-contract.txt")?,MediaType::new("text/plain")?),
  ComponentIntegrity::new(payload.len() as u64,digest(&payload),None),
  ComponentOwnership::new(Owner::new("real-testing")?,Provenance::new("actual CLI executable content digest")?),
  ComponentRequirements::new(vec![],CompatibilityContract::new(SchemaInterval::new(schema,schema)?,vec![],vec![])?,AuthoritySet::empty(),kind.protection_class()),HarnessLimits::compiled())?;
 let content=VerifiedComponentContent::new(&declaration,payload)?;
 let graph=CheckedHarnessGraph::check(vec![declaration],&GraphEnvironment::new(vec![],vec![])?,HarnessLimits::compiled())?;
 let contents=ComponentContents::new(&graph,vec![content])?;
 let manifest=ManifestDigest::new(digest(&graph.canonical_bytes()));
 Ok(match previous {None=>HarnessRevision::genesis(LineageSeed::new(digest(b"rsi actual CLI corpus")),manifest,graph,&contents)?,Some(p)=>HarnessRevision::successor(p,manifest,graph,&contents)?})
}
#[derive(Serialize,Deserialize)]struct Task {args:Vec<String>}
#[derive(Serialize,Deserialize)]struct Output {exit:i32,stdout:String,stderr:String}
struct Port<'a>{store:&'a ArtifactStore,baseline:PathBuf,candidate:PathBuf,root:PathBuf,evaluator:PathBuf,candidate_calls:u32,evaluator_calls:u32,fail_evaluator:bool}
impl Port<'_>{
 fn failure(class:InfrastructureFailureClass,e:impl std::fmt::Display)->ExecutionFailure {ExecutionFailure::new(class,digest(e.to_string().as_bytes()),false)}
 fn resources(start:Instant)->ResourceObservation {ResourceObservation::new(start.elapsed().as_micros() as u64,None,None,None,None,None,None,true,true)}
}
impl RolloutExecutionPort for Port<'_>{
 fn execute_candidate(&mut self,d:&CandidateExecutionDirective)->Result<CandidateObservation,ExecutionFailure>{
  self.candidate_calls+=1;let start=Instant::now();
  let run=||->AnyResult<(StoredDigest,u64)> {
   let input:Task=serde_json::from_slice(&read(self.store,d.input().artifact())?)?;
   let executable=match d.arm(){EvaluationArm::Baseline=>&self.baseline,EvaluationArm::Candidate=>&self.candidate};
   let output=Command::new("timeout").arg("5").arg(executable).args(input.args).current_dir(&self.root).env("XDG_CONFIG_HOME",self.root.join("candidate-config")).env("XDG_STATE_HOME",self.root.join("candidate-state")).output()?;
   require(!matches!(output.status.code(),Some(124|126|127)),"actual candidate execution unavailable or exceeded deadline")?;
   let payload=serde_json::to_vec(&Output{exit:output.status.code().unwrap_or(-1),stdout:String::from_utf8(output.stdout)?,stderr:String::from_utf8(output.stderr)?})?;
   let artifact=save(self.store,&payload)?; Ok((artifact,payload.len() as u64))
  };
  let (output,len)=run().map_err(|e|Self::failure(InfrastructureFailureClass::Execution,e))?;
  CandidateObservation::new(d.rollout_id(),d.attempt(),d.request_digest(),output,len,d.execution().digest(),d.provider().digest(),Self::resources(start)).map_err(|e|Self::failure(InfrastructureFailureClass::Execution,e))
 }
 fn execute_evaluator(&mut self,d:&EvaluatorExecutionDirective)->Result<EvaluatorObservation,ExecutionFailure>{
  self.evaluator_calls+=1;let start=Instant::now();
  let run=||->AnyResult<(EvaluatorVerdict,Sha256Digest)> {
   let input=self.root.join("evaluator-only-input.json");let output=self.root.join("finalized-candidate-output.json");
   fs::write(&input,read(self.store,d.sealed_input().artifact())?)?;fs::write(&output,read(self.store,d.candidate_output())?)?;
   let evaluator=if self.fail_evaluator {self.root.join("missing-evaluator.py")}else {self.evaluator.clone()};
   let observation=Command::new("python3").arg(evaluator).arg(&input).arg(&output).output()?;
   require(matches!(observation.status.code(),Some(0|1)),"evaluator execution failure is not a task verdict")?;
   let result:serde_json::Value=serde_json::from_slice(&observation.stdout)?;
   let passed=result["passed"].as_bool().ok_or("evaluator omitted valid verdict")?;
   let artifact=save(self.store,&observation.stdout)?;
   fs::write(self.root.join(format!("evaluator-{:?}.json",d.rollout_id())),&observation.stdout)?;
   Ok((if passed {EvaluatorVerdict::Passed}else {EvaluatorVerdict::Failed(TaskFailureClass::Incorrect)},artifact.sha256()))
  };
  let (verdict,result)=run().map_err(|e|Self::failure(InfrastructureFailureClass::Evaluator,e))?;
  EvaluatorObservation::new(d.rollout_id(),d.attempt(),d.request_digest(),d.candidate_output(),verdict,result,d.execution().digest(),Self::resources(start)).map_err(|e|Self::failure(InfrastructureFailureClass::Evaluator,e))
 }
}
struct Cancel;impl CancellationProbe for Cancel{fn cancelled(&self,_:RolloutId)->bool{true}}
fn command(state:Option<&EvaluationState>,campaign:EvaluationCampaignId,profile:ProfileDigest,n:u8,kind:EvaluationCommandKind)->AnyResult<EvaluationCommand>{
 Ok(EvaluationCommand::new(CommandId::new([n;16]).unwrap(),eid(n),campaign,state.map_or(0,EvaluationState::sequence),state.map(EvaluationState::last_event_id),state.map_or(Sha256Digest::new([0;32]),EvaluationState::state_digest),profile,kind)?)
}
fn main()->AnyResult<()>{
 let args=std::env::args().collect::<Vec<_>>();require(args.len()==5,"usage: consumer baseline candidate evidence-root evaluator.py")?;
 let root=PathBuf::from(&args[3]);fs::create_dir_all(&root)?;
 let db=root.join("peritus.sqlite3");let storeid=StoreId::new([221;16])?;
 let mut journal=SqliteJournal::open(&db,storeid,SqliteJournalOptions::default())?;
 let store=ArtifactStore::open(StoreConfig::new(root.join("artifacts"),16*1024*1024,128*1024*1024)?.with_database_path(&db)?)?;
 let baseline=revision(Path::new(&args[1]),None)?;let candidate=revision(Path::new(&args[2]),Some(&baseline))?;
 let provider_id=ProviderProfileId::new([204;16]).unwrap();
 let revision=RevisionTuple::new(AcceptanceSpecId::new([202;16]).unwrap(),baseline.harness_id(),WorkspaceId::new([203;16]).unwrap(),Generation::new(1).unwrap(),RevisionNumber::new(1).unwrap(),PolicyId::new([205;16]).unwrap(),provider_id);
 // The frozen provider is inert local-process metadata, not an actual model request.
 let provider=FrozenProviderSnapshot::capture(&ProviderProfile::new(provider_id,1,ProviderName::new("local-consumer".to_owned())?,ModelName::new("cli-process".to_owned())?,WireDialect::CompatibleChatCompletions,CapabilityMatrix::new(&[],&[])?,CapabilityProvenance::Profiled,ModelLimits::new(1024,1024,1,1,1024)?,OutputLimitEnforcement::Advisory,StateMode::StatelessReplay,ResumeKind::Unsupported,CancellationKind::Unsupported)?)?;
 let mut tasks=vec![];let oracle=fs::read(&args[4])?;
 for (i,(kind,args)) in [("version",vec!["--version"]),("help",vec!["--help"]),("bad-argument",vec!["--this-argument-is-invalid-rsi"])].into_iter().enumerate(){
  let public=serde_json::to_vec(&Task{args:args.into_iter().map(str::to_owned).collect()})?;
  let sealed=serde_json::to_vec(&serde_json::json!({"kind":kind,"version":"0.0.7"}))?;
  tasks.push(DatasetTask::new(TaskId::new([i as u8+1;16])?,DatasetPartition::Regression,1,CandidateTaskInput::new(save(&store,&public)?,public.len() as u64)?,SealedEvaluatorInput::new(save(&store,&sealed)?,sealed.len() as u64,digest(&oracle))?,digest(b"one local CLI process, deadline five seconds"))?);
 }
 let limits=EvaluationLimits::production();let dataset=DatasetManifest::new(DatasetId::new([211;16])?,1,tasks,digest(b"Cargo package version0.0.7 and public CLI help/error contract"),limits)?;
 let model=FrozenModelControls::new(1024,None,None,digest(b"actual local CLI invocation; no provider request"),SeedDeliveryPolicy::RecordedOnly,provider)?;
 let resources=ResourceVector::new(vec![ResourceEntry::new(ResourceKind::CPU,ResourceQuantity::new(1)?)],1)?;
 let execution=ExecutionBinding::new(digest(b"actual unsandboxed process consumer"),digest(b"no restricted isolation claimed"),digest(std::env::consts::OS.as_bytes()),digest(&fs::read(&args[1])?),resources,5_000_000,1,false,true)?;
 let profile=FrozenEvaluationProfile::new(dataset.clone(),vec![DatasetPartition::Regression],HarnessArmBinding::new(revision,baseline.identity(),digest(&fs::read(&args[1])?)),HarnessArmBinding::new(revision,candidate.identity(),digest(&fs::read(&args[2])?)),provider,model,execution,EvaluationRetryPolicy::new(2,0,0,limits)?,MetricPolicy::new(vec![1,2],100,950_000,100_000,false,limits)?,InfrastructurePolicy::new(InfrastructureTreatment::InvalidateMetric,InfrastructureTreatment::CountAsFailure,InfrastructureTreatment::ExcludeWithDenominator),2,false,limits)?;
 let campaign=EvaluationCampaignId::new([212;16]).unwrap();let plan=EvaluationPlan::build(campaign,&profile)?;
 let mut ledger=RolloutLedger::from_plan(&plan,2);
 require(analyze_evaluation(&plan,&profile,&ledger).is_err(),"incomplete evaluation was analyzed")?;
 let mut port=Port{store:&store,baseline:PathBuf::from(&args[1]),candidate:PathBuf::from(&args[2]),root:root.clone(),evaluator:PathBuf::from(&args[4]),candidate_calls:0,evaluator_calls:0,fail_evaluator:false};
 let mut observed=vec![];
 for spec in plan.specs(){
  let actual=execute_rollout(&mut port,&NeverCancelled,spec,&profile,1)?;
  let record=RolloutRecord::from_execution(spec,actual,None,None)?;
  observed.push(serde_json::json!({"task":format!("{:?}",spec.task_id()),"arm":format!("{:?}",spec.arm()),"ordinal":spec.ordinal(),"outcome":format!("{:?}",record.outcome())}));
  ledger.record_attempt(spec.id(),actual.attempt())?;ledger.settle(record)?;ledger.settle(record)?;
 }
 require(ledger.complete(),"complete executed matrix did not conserve terminals")?;
 if std::env::var_os("EXPECT_VERSION_FIX").is_some(){
  require(ledger.counts().passed==10 && ledger.counts().task_failed==2,"actual baseline version defect or candidate repair not detected")?;
  for record in ledger.records(){
   let expected=record.task_id()!=TaskId::new([1;16])? || record.arm()==EvaluationArm::Candidate;
   require(record.outcome().passed()==expected,"observed task/arm verdict differs from independently known seeded defect")?;
  }
 }
 let analysis=analyze_evaluation(&plan,&profile,&ledger)?;
 let report=EvaluationReport::new(campaign,dataset.digest(),profile.digest(),plan.id(),plan.digest(),analysis,None)?.validate()?;
 let artifact=finalize_report_artifact(&store,&report,eid(201))?;
 require(read(&store,artifact.artifact_digest())?==report.bytes(),"saved report differs from canonical bytes")?;
 let calls=(port.candidate_calls,port.evaluator_calls);let cancelled=execute_rollout(&mut port,&Cancel,&plan.specs()[0],&profile,1)?;
 require(cancelled.attempt().terminal()==RolloutOutcome::Cancelled && calls==(port.candidate_calls,port.evaluator_calls),"pre-effect cancellation executed a process")?;
 port.fail_evaluator=true;let outage=execute_rollout(&mut port,&NeverCancelled,&plan.specs()[0],&profile,1)?;
 require(matches!(outage.attempt().terminal(),RolloutOutcome::InfrastructureFailed{class:InfrastructureFailureClass::Evaluator,..}),"evaluator outage became task failure")?;
 // Real C0 lifecycle exercised without invented D3 scheduling acknowledgements.
 let dataset_artifact=save(&store,&dataset.canonical_bytes()?)?;
 let profile_artifact=save(&store,format!("{:?}",profile).as_bytes())?;
 let create=command(None,campaign,profile.digest(),230,EvaluationCommandKind::CreateCampaign{revision,dataset_digest:dataset.digest(),dataset_artifact,profile_artifact})?;
 let transition=decide(None,&create)?;commit_evaluation_transition(&mut journal,&create,&transition)?;let state=transition.state();
 let cancel=command(Some(state),campaign,profile.digest(),231,EvaluationCommandKind::CancelCampaign{reason_digest:digest(b"operator ended disposable campaign before scheduling")})?;
 let transition=decide(Some(state),&cancel)?;commit_evaluation_transition(&mut journal,&cancel,&transition)?;
 let finish=command(Some(transition.state()),campaign,profile.digest(),232,EvaluationCommandKind::CompleteCancellation)?;
 let transition=decide(Some(transition.state()),&finish)?;commit_evaluation_transition(&mut journal,&finish,&transition)?;

 let expected_cancelled=transition.state().clone();
 let scheduling_campaign=EvaluationCampaignId::new([213;16]).unwrap();
 let scheduling_plan=EvaluationPlan::build(scheduling_campaign,&profile)?;
 let create=command(None,scheduling_campaign,profile.digest(),240,EvaluationCommandKind::CreateCampaign{revision,dataset_digest:dataset.digest(),dataset_artifact,profile_artifact})?;
 let transition=decide(None,&create)?;commit_evaluation_transition(&mut journal,&create,&transition)?;
 let mut bindings=scheduling_plan.specs().iter().map(|s|PlannedRolloutBinding::new(s.id(),s.work_id(),s.request_digest())).collect::<Vec<_>>();
 bindings.sort_by_key(|b|b.rollout_id());
 let plan_bytes=format!("{:?}",scheduling_plan).into_bytes();let plan_artifact=save(&store,&plan_bytes)?;
 let batch=PlanBatch::new(1,1,plan_artifact,bindings)?;
 let record=command(Some(transition.state()),scheduling_campaign,profile.digest(),241,EvaluationCommandKind::RecordPlanBatch{plan_id:scheduling_plan.id(),plan_digest:scheduling_plan.digest(),batch})?;
 let transition=decide(Some(transition.state()),&record)?;commit_evaluation_transition(&mut journal,&record,&transition)?;
 let plan_record=PlanRecord::new(scheduling_plan.id(),scheduling_plan.digest(),plan_artifact,scheduling_plan.specs().len() as u32,1)?;
 let complete=command(Some(transition.state()),scheduling_campaign,profile.digest(),242,EvaluationCommandKind::CompletePlan{plan:plan_record})?;
 let transition=decide(Some(transition.state()),&complete)?;commit_evaluation_transition(&mut journal,&complete,&transition)?;
 let work=scheduling_plan.specs()[0].work_spec(ActorId::new([214;16]).unwrap(),revision,profile.execution().resource_request().clone(),2,peritus_scheduler::SchedulerLimits::new(32,64,2,8,8,2,2,4,2,65536,1024*1024)?)?;
 let request=command(Some(transition.state()),scheduling_campaign,profile.digest(),243,EvaluationCommandKind::RequestSchedule{rollout_id:scheduling_plan.specs()[0].id(),work})?;
 let scheduling_transition=decide(Some(transition.state()),&request)?;commit_evaluation_transition(&mut journal,&request,&scheduling_transition)?;
 fs::write(root.join("schedule-request.txt"),format!("Actual E3 campaign {:?} phase {:?} with durable schedule outbox. No D3 acknowledgment or task result fabricated.\n",scheduling_campaign,scheduling_transition.state().phase()))?;
 let actual_candidate_calls=port.candidate_calls;let actual_evaluator_calls=port.evaluator_calls;drop(port);
 let expected=expected_cancelled;drop(journal);drop(store);
 let reopened=SqliteJournal::open(&db,storeid,SqliteJournalOptions::default())?;
 require(load_evaluation_replay(&reopened,campaign)?.rebuild()?.as_ref()==Some(&expected),"fresh journal replay differs")?;
 let store=ArtifactStore::open(StoreConfig::new(root.join("artifacts"),16*1024*1024,128*1024*1024)?.with_database_path(&db)?)?;
 require(read(&store,artifact.artifact_digest())?==report.bytes(),"fresh artifact consumer readback differs")?;
 require(peritus_evolution::recover_campaign(&reopened,EvolutionCampaignId::new([219;16]).unwrap())?.state().is_none(),"F0 fabricated campaign from absence")?;
 require(peritus_evolution::recover_pointer(&reopened,ProjectId::new([220;16]).unwrap())?.state().is_none(),"F0 fabricated production pointer from absence")?;
 let results=serde_json::json!({"executable_baseline":args[1],"executable_candidate":args[2],"rollouts":observed,"counts":format!("{:?}",ledger.counts()),"baseline":format!("{:?}",report.report().analysis().baseline()),"candidate":format!("{:?}",report.report().analysis().candidate()),"paired":format!("{:?}",report.report().analysis().paired()),"report_artifact":format!("{:?}",artifact.artifact_digest()),"candidate_calls":actual_candidate_calls,"evaluator_calls":actual_evaluator_calls,"pre_execution_cancel":"PASS","evaluator_outage_classification":"PASS","incomplete_analysis_rejection":"PASS","duplicate_settlement":"PASS","canonical_report_readback_restart":"PASS","sqlite_campaign_cancellation_replay":"PASS","limits":"Library execution consumer only. No production E1 receipt, C2/C3 restricted isolation, provider request, D3 runtime, F0 production promotion or operator wiring claimed."});
 fs::write(root.join("results.json"),serde_json::to_vec_pretty(&results)?)?;
 println!("{}",serde_json::to_string_pretty(&results)?);Ok(())
}
