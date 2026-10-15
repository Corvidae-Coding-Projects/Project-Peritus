use peritus_model_protocol::{ContentBlock, ProtocolLimits, decode_messages};
use std::{error::Error, fs, path::Path};
fn scan(path: &Path, out: &Path, count: &mut usize) -> Result<(), Box<dyn Error>> {
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            scan(&entry?.path(), out, count)?;
        }
    } else if let Ok(messages) = decode_messages(&fs::read(path)?, ProtocolLimits::PRODUCTION) {
        if messages.iter().flat_map(|m|m.content()).any(|b|matches!(b,ContentBlock::Text(t) if t.expose_for_wire().contains("prompt-vitamins-v1"))) {
   let view:Vec<_>=messages.iter().map(|m|serde_json::json!({"role":format!("{:?}",m.role()),"text":m.content().iter().filter_map(|b|if let ContentBlock::Text(t)=b {Some(t.expose_for_wire())}else{None}).collect::<Vec<_>>() })).collect();
   fs::write(out.join(format!("view-{:05}.json",*count)),serde_json::to_vec_pretty(&serde_json::json!({"source":path,"messages":view}))?)?;*count+=1;
  }
    }
    Ok(())
}
fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, root, out] = args.as_slice() else {
        return Err("usage: archive-consumer OWNED_CONTEXT OUTPUT_DIRECTORY".into());
    };
    let root = Path::new(root);
    let out = Path::new(out);
    fs::create_dir_all(out)?;
    let mut count = 0;
    scan(root, out, &mut count)?;
    println!("{}", serde_json::json!({"decoded_prompt_views":count}));
    Ok(())
}
