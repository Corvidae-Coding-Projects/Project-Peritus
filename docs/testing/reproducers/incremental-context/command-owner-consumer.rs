//! Public C4/C2 owner operations shared by previews and developer commands, with real processes.
mod application {
    include!("../checkpoint-cache/context-consumer.rs");
    use std::io::{BufRead as _, Write as _};

    fn render(value: peritus_product_runner::PreviewObservation) -> Value {
        json!({"state":format!("{:?}",value.state()),"stdout":value.stdout(),
            "stderr":value.stderr(),"exit_code":value.exit_code()})
    }
    fn owner(spec: &Value) -> Result<peritus_product_runner::PreviewOwner> {
        let value: Value =
            serde_json::from_slice(&fs::read(spec["owner_file"].as_str().ok_or("owner_file")?)?)?;
        fn id(v: &Value) -> Result<[u8; 16]> {
            let text = v.as_str().ok_or("identity required")?;
            if text.len() != 32 {
                return Err("bad identity size".into());
            }
            let mut bytes = [0; 16];
            for (i, b) in bytes.iter_mut().enumerate() {
                *b = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16)?;
            }
            Ok(bytes)
        }
        Ok(peritus_product_runner::PreviewOwner::new(
            RunId::new(id(&value["source_run_id"])?).map_err(|e| format!("{e:?}"))?,
            RunId::new(id(&value["run_id"])?).map_err(|e| format!("{e:?}"))?,
            peritus_types::ActionId::new(id(&value["action_id"])?).map_err(|e| format!("{e:?}"))?,
            peritus_types::ProcessId::new(id(&value["process_id"])?)
                .map_err(|e| format!("{e:?}"))?,
        ))
    }
    pub fn run(spec: Value) -> Result<()> {
        let input = input(&spec)?;
        let runtime = input.command_runtime;
        let mut launch = None;
        for line in std::io::stdin().lock().lines() {
            let request: Value = serde_json::from_str(&line?)?;
            let answer = (|| -> Result<Value> {
                if request["action"] == "start" {
                    if launch.is_some() {
                        return Err("fixture refuses a second start".into());
                    }
                    let command = peritus_product_runner::PreviewCommand::new(
                        "/usr/bin/python3".to_owned(),
                        vec!["worker.py".to_owned()],
                        input.workspace_root.clone(),
                        Duration::from_secs(90),
                        spec["interactive"].as_bool().unwrap_or(false),
                        24,
                        80,
                        "owned-crash-716294".to_owned(),
                        Vec::new(),
                    )?;
                    let mut registered=|owner:peritus_product_runner::PreviewOwner|->std::result::Result<(),String>{
                        let hex=|b:&[u8]|b.iter().map(|b|format!("{b:02x}")).collect::<String>();
                        let value=json!({"source_run_id":hex(owner.source_run().as_bytes()),"run_id":hex(owner.execution_run().as_bytes()),
                            "action_id":hex(owner.action().as_bytes()),"process_id":hex(owner.process().as_bytes())});
                        let path=PathBuf::from(spec["owner_file"].as_str().ok_or("owner_file")?);
                        let mut file=fs::File::create(&path).map_err(|e|e.to_string())?;
                        file.write_all(value.to_string().as_bytes()).and_then(|()|file.sync_all()).map_err(|e|e.to_string())?;
                        #[cfg(unix)] fs::File::open(path.parent().ok_or("owner parent")?).and_then(|f|f.sync_all()).map_err(|e|e.to_string())?;
                        Ok(())
                    };
                    launch = Some(runtime.launch_preview_registered(&command, &mut registered)?);
                } else if launch.is_none() {
                    launch = Some(runtime.reconnect_preview(owner(&spec)?)?);
                }
                let current = launch.as_ref().ok_or("no launch")?;
                let value = match request["action"].as_str().ok_or("action")? {
                    "start" | "poll" | "recover" => runtime.observe_preview(current)?,
                    "stdin" => runtime.interact_preview(
                        current,
                        request["text"].as_str().ok_or("text")?.as_bytes().to_vec(),
                    )?,
                    "cancel" => runtime.stop_preview(current)?,
                    _ => return Err("unknown action".into()),
                };
                Ok(render(value))
            })();
            println!(
                "{}",
                match answer {
                    Ok(value) => json!({"ok":true,"value":value}),
                    Err(e) => json!({"ok":false,"error":e.to_string()}),
                }
            );
            std::io::stdout().flush()?;
        }
        Ok(())
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    application::run(serde_json::from_slice(&std::fs::read(
        std::env::args().nth(1).ok_or("spec required")?,
    )?)?)
}
