//! Actual scoped comparison, role evidence projection and exact source paging consumer.
use peritus_model_protocol::{BoundedText, ProtocolLimits};
use peritus_product_runner::{ScopedBaseline, context_projection::{DeveloperEvidencePrompt, DeveloperEvidenceSources, evidence_tool_definition, project_developer_evidence}};
use serde_json::{Value,json};
use sha2::{Digest,Sha256};
use std::{collections::BTreeSet,error::Error,fs,path::{Path,PathBuf}};

fn digest(value: &str) -> String { Sha256::digest(value.as_bytes()).iter().map(|b|format!("{b:02x}")).collect() }
fn exact_source_read(source: &str, section: &str, sources: &DeveloperEvidenceSources) -> Result<(bool,usize),Box<dyn Error>> {
    let mut bytes=String::new();let mut offset=0_u64;let mut pages=0;
    loop {
        let page=sources.read(section,offset,4096)?;
        if page["section"]!=section || page["offset"]!=offset || page["total_bytes"]!=source.len() as u64 || page["sha256"]!=digest(source) {return Ok((false,pages));}
        let wire=serde_json::to_string(&page)?;
        if BoundedText::new(wire,ProtocolLimits::PRODUCTION).is_err() {return Ok((false,pages));}
        bytes.push_str(page["bytes"].as_str().ok_or("page has no text bytes")?);pages+=1;
        match page["next_offset"].as_u64() {Some(next) if next>offset=>offset=next,None=>break,_=>return Ok((false,pages))}
    }
    Ok((bytes==source && sources.section(section)==Some(source),pages))
}
fn projection_case(root:&Path,case:&str,source_diff:&str,source_gates:&str,budget:u64) -> Result<Value,Box<dyn Error>> {
    let tools=vec![evidence_tool_definition()?];
    let sources=DeveloperEvidenceSources::new("User requirement résumé 雪".into(),source_diff.into(),source_gates.into(),String::new(),String::new(),String::new());
    let (diff_exact,diff_pages)=exact_source_read(source_diff,"diff",&sources)?;
    let (gate_exact,gate_pages)=exact_source_read(source_gates,"gates",&sources)?;
    let projected=project_developer_evidence(&DeveloperEvidencePrompt {
        system:"Act on the requested source repair.",instructions:"Preserve the requested source and check its actual output.",
        tools:&tools,diff:source_diff,gates:source_gates,max_input_tokens:budget,additional_framing_tokens:0,
    });
    let (text_ok,identity_ok,bytes,nuls,error)=match projected {
        Ok(text)=>{
            let b=text.len();let n=text.bytes().filter(|v| *v==0).count();
            let identity=[("diff",source_diff),("gates",source_gates)].into_iter().all(|(section,source)| {
                !source.contains('\0') || (text.contains(&format!("section={section}")) && text.contains(&format!("original_bytes={}",source.len())) && text.contains(&digest(source))
                  && text.contains(&format!("exact omitted UTF-8 byte range=[0,{}).",source.len())))
            });
            fs::write(root.join(format!("{case}-{budget}.prompt.txt")),&text)?;
            match BoundedText::new(text,ProtocolLimits::PRODUCTION) {Ok(_)=>(true,identity,b,n,None),Err(e)=>(false,identity,b,n,Some(e.to_string()))}
        },
        Err(e)=>(false,false,0,0,Some(e.to_string())),
    };
    Ok(json!({"case":case,"budget":budget,"ok":text_ok && identity_ok && diff_exact && gate_exact,"message_valid":text_ok,"source_identity_preserved":identity_ok,"source_exact_readback":diff_exact && gate_exact,"retrieval_pages":diff_pages+gate_pages,"prompt_bytes":bytes,"prompt_nuls":nuls,"error":error}))
}
fn main() -> Result<(),Box<dyn Error>> {
    let arguments=std::env::args().skip(1).collect::<Vec<_>>();
    let root=PathBuf::from(arguments.first().ok_or("usage: consumer NEW_DISPOSABLE_ROOT [SAVED_LIVE_DIFF]")?);
    fs::create_dir(&root)?;let project=root.join("project");fs::create_dir(&project)?;fs::create_dir(project.join("runtime"))?;
    fs::write(project.join("main.txt"),"SOURCE BEFORE\n")?;
    fs::write(project.join("runtime/original-page.bin"),vec![0_u8;20000])?;
    let scope=ScopedBaseline::new(project.clone(),root.join("journal.jsonl"),vec![],1);
    scope.enroll("runtime")?;scope.enroll("main.txt")?;
    fs::write(project.join("runtime/page.bin"),vec![0_u8;8192])?;
    fs::write(project.join("main.txt"),"SOURCE AFTER résumé 雪\n")?;
    let paths=scope.changed_paths(&project)?;let diff=scope.diff(&project)?;fs::write(root.join("actual-scoped.diff"),&diff)?;
    let text=BoundedText::new(diff.clone(),ProtocolLimits::PRODUCTION);
    let mut observations=vec![json!({"case":"scoped-binary-page-diff","ok":text.is_ok() && paths.contains(&PathBuf::from("runtime/page.bin")) && paths.contains(&PathBuf::from("main.txt")) && diff.contains("SOURCE AFTER résumé 雪"),"paths":paths,"diff_bytes":diff.len(),"diff_nuls":diff.bytes().filter(|v| *v==0).count(),"error":text.err().map(|e|e.to_string())})];
    // Mutate beyond the preview prefix, reopen the actual durable baseline, then restore.
    let mut modified=vec![0_u8;20000];modified[19000]=65;fs::write(project.join("runtime/original-page.bin"),&modified)?;
    let reopened:ScopedBaseline=serde_json::from_slice(&serde_json::to_vec(&scope)?)?;
    let changed=reopened.changed_paths(&project)?.contains(&PathBuf::from("runtime/original-page.bin"));
    let legacy_diff=reopened.diff(&project)?;
    let legacy_render_safe=legacy_diff.contains("runtime/original-page.bin") && BoundedText::new(legacy_diff,ProtocolLimits::PRODUCTION).is_ok();
    fs::write(project.join("runtime/original-page.bin"),vec![0_u8;20000])?;
    let restored=!reopened.changed_paths(&project)?.contains(&PathBuf::from("runtime/original-page.bin"));
    observations.push(json!({"case":"binary-beyond-preview-freshness-and-restoration","ok":changed && restored && legacy_render_safe,"changed_beyond_preview":changed,"original_bytes_restore_baseline":restored,"nul_bearing_baseline_render_safe":legacy_render_safe}));
    for (case,d,g) in [("role-projection-scoped-diff",diff.as_str(),"real check exit0\n"),("role-projection-nul-diff","left\0right résumé 雪","real check exit0\n"),("role-projection-nul-gate-output","source change ASCII résumé 雪","before\0after"),("role-projection-ordinary-text","left right résumé 雪","before after\n")] {observations.push(projection_case(&root,case,d,g,200000)?);}
    for size in [1_usize,65,8192] {
        for position in BTreeSet::from([0,size/2,size-1]) {
            let mut bytes=vec![b'x';size];bytes[position]=0;let mut value=String::from_utf8(bytes)?;value.push_str(" résumé 雪");
            for budget in [8192,200000] {observations.push(projection_case(&root,&format!("generated-size{size}-nul{position}"),&value,"actual gate0\n",budget)?);}
        }
    }
    if let Some(path)=arguments.get(1) {let live=fs::read_to_string(path)?;observations.push(projection_case(&root,"saved-live-diff",&live,"actual gates retained separately\n",200000)?);}
    observations.push(json!({"case":"protocol-still-rejects-raw-nul","ok":BoundedText::new("raw\0text".into(),ProtocolLimits::PRODUCTION).is_err()}));
    let passed=observations.iter().all(|row|row["ok"]==true);let result=json!({"passed":passed,"case_count":observations.len(),"cases":observations});
    fs::write(root.join("result.json"),serde_json::to_vec_pretty(&result)?)?;println!("{}",serde_json::to_string_pretty(&result)?);
    if !passed {std::process::exit(1);} Ok(())
}
