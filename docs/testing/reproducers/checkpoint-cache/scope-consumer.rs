//! Public API consumer; the Python driver independently owns all fixture mutations.
use peritus_product_runner::ScopedBaseline;
use serde_json::{Value,json};
use std::{error::Error,fs,io::{self,BufRead,Write},path::PathBuf,time::Instant};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
fn read_bytes() -> u64 {
    fs::read_to_string("/proc/self/io").ok().and_then(|text|text.lines().find_map(|line|line.strip_prefix("rchar: ").and_then(|n|n.trim().parse().ok()))).unwrap_or(0)
}
fn main()->Result<()> {
    let args=std::env::args().collect::<Vec<_>>();
    let root=fs::canonicalize(args.get(1).ok_or("root required")?)?;
    let journal=PathBuf::from(args.get(2).ok_or("journal required")?);
    let scope=ScopedBaseline::new(root.clone(),journal,Vec::new(),1);
    println!("{}",json!({"ready":true}));io::stdout().flush()?;
    for line in io::stdin().lock().lines(){
        let line=line?;let request:Value=serde_json::from_str(&line)?;
        let started=Instant::now();let before=read_bytes();
        let operation:Result<Value>=(||{
          match request["op"].as_str().ok_or("op required")? {
            "enroll"=>{scope.enroll(request["path"].as_str().ok_or("path required")?)?;Ok(json!({"enrolled":true}))},
            "observe"=>{let count=request["repeat"].as_u64().unwrap_or(1);let mut paths=Vec::new();for _ in 0..count {paths=scope.changed_paths(&root)?;}Ok(json!({"paths":paths,"repeat":count}))},
            "diff"=>Ok(json!({"diff":scope.diff(&root)?})),
            "paths"=>Ok(json!({"paths":scope.paths()?})),
            "wrong-root"=>{let result=scope.changed_paths(&root.join("other"));Ok(json!({"rejected":result.is_err()}))},
            "parallel"=>{let readers=request["readers"].as_u64().unwrap_or(4);let mut workers=Vec::new();for _ in 0..readers {let scope=scope.clone();let root=root.clone();workers.push(std::thread::spawn(move ||scope.changed_paths(&root)));}let mut results=Vec::new();for worker in workers {results.push(worker.join().map_err(|_|"worker panicked")??);}Ok(json!({"results":results}))},
            _=>Err("unknown op".into())
          }
        })();
        let after=read_bytes();let result=match operation {Ok(value)=>json!({"ok":true,"value":value,"logical_read_bytes":after.saturating_sub(before),"elapsed_ms":started.elapsed().as_secs_f64()*1000.0}),Err(error)=>json!({"ok":false,"error":error.to_string()})};
        println!("{result}");io::stdout().flush()?;
    }
    Ok(())
}
