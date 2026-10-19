use peritus_provider_core::{CancellationToken,ProcessExecutable,ProcessLimits,ProcessRequest,ProcessTransport,TokioProcessTransport,ProviderCoreErrorKind};
use std::{error::Error,path::PathBuf,sync::{Arc,Mutex,atomic::{AtomicBool,Ordering}},collections::VecDeque,time::{Duration,Instant}};
#[tokio::main(flavor="current_thread")]
async fn main()->Result<(),Box<dyn Error>>{
 let root=PathBuf::from(std::env::args().nth(1).ok_or("output directory")?);std::fs::create_dir_all(&root)?;
 for mode in ["ordinary","queued","oversize","timeout","cancel","stdout-limit"] {
  let pidfile=root.join(format!("{mode}.pid"));let code="import os,sys,time,json\nopen(sys.argv[2],'w').write(str(os.getpid()))\nmode=sys.argv[1]\nif mode=='ordinary':\n print(sys.stdin.read());sys.exit(0)\nprint('READY',flush=True)\nif mode in ('timeout','cancel'):time.sleep(60)\nif mode=='stdout-limit':print('X'*65536,flush=True);time.sleep(60)\ndata=sys.stdin.buffer.read()\nprint(json.dumps({'bytes':len(data),'sum':sum(data)}),flush=True)";
  let queue=Arc::new(Mutex::new(VecDeque::new()));let close=Arc::new(AtomicBool::new(false));let once=Arc::new(AtomicBool::new(false));let(q,c,o)=(queue.clone(),close.clone(),once.clone());
  let timeout=if mode=="timeout"{Duration::from_millis(300)}else{Duration::from_secs(3)};
  let input_limit=if mode=="oversize"{1024}else{1<<20};let out_limit=if mode=="stdout-limit"{1024}else{1<<20};
  let mut req=ProcessRequest::new(ProcessExecutable::pin("/usr/bin/python3")?,vec!["-c".into(),code.into(),mode.into(),pidfile.display().to_string()],if mode=="ordinary"{b"ordinary-pipe-EOF".to_vec()}else{Vec::new()},None,Vec::new(),ProcessLimits::new(input_limit,out_limit,4096,timeout)?)?;
  if mode!="ordinary" {req=req.with_stdin_messages(queue).with_stdin_close_signal(close).with_stdout_observer(Arc::new(move|bytes|{
   if bytes.windows(5).any(|x|x==b"READY")&&!o.swap(true,Ordering::AcqRel){
    if mode=="queued" {let mut queue=q.lock().unwrap();queue.push_back(vec![b'A';100000]);queue.push_back(vec![b'B';100000]);queue.push_back(b"END\n".to_vec());c.store(true,Ordering::Release);}
    if mode=="oversize" {q.lock().unwrap().push_back(vec![b'Z';2048]);}
   }
  }));}
  let cancel=CancellationToken::new();if mode=="cancel"{let signal=cancel.clone();tokio::spawn(async move{tokio::time::sleep(Duration::from_millis(250)).await;let _ = signal.cancel();});}
  let started=Instant::now();let result=TokioProcessTransport.run(req,&cancel).await;
  match mode {
   "ordinary"=>{let result=result?;assert!(result.exit().success());assert_eq!(result.stdout(),b"ordinary-pipe-EOF\n");},
   "queued"=>{let result=result?;assert!(result.exit().success());let value:serde_json::Value=serde_json::from_slice(result.stdout().split(|x|*x==b'\n').nth(1).unwrap())?;assert_eq!(value["bytes"],200004);assert_eq!(value["sum"],13100225);},
   _=>{let e=result.expect_err("owned failure must surface");match mode{"oversize"|"stdout-limit"=>assert_eq!(e.kind(),ProviderCoreErrorKind::LimitExceeded),"timeout"=>assert_eq!(e.operation(),"process_timeout"),"cancel"=>assert_eq!(e.kind(),ProviderCoreErrorKind::Cancelled),_=>unreachable!()};}
  }
  let pid=std::fs::read_to_string(&pidfile)?;assert!(!PathBuf::from(format!("/proc/{pid}")).exists(),"owned child must be reaped");
  println!("{}",serde_json::json!({"case":mode,"pass":true,"seconds":started.elapsed().as_secs_f64(),"child_reaped":true}));
 }
 Ok(())
}
