//! A command-line host of the actual public scoped-baseline API.
use peritus_product_runner::ScopedBaseline;
use serde_json::{Value, json};
use std::{error::Error, fs, path::PathBuf};

fn main() {
    if let Err(error) = run() {
        println!("{}", json!({"ok": false, "error": error.to_string()}));
        std::process::exit(2);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let operation = arguments.first().ok_or("expected operation")?;
    if operation == "new" {
        if arguments.len() != 6 {
            return Err("new ROOT JOURNAL REVISION PROTECTED_JSON DESCRIPTOR".into());
        }
        let protected: Vec<PathBuf> = serde_json::from_str(&arguments[4])?;
        let baseline = ScopedBaseline::new(
            PathBuf::from(&arguments[1]),
            PathBuf::from(&arguments[2]),
            protected,
            arguments[3].parse()?,
        );
        fs::write(&arguments[5], serde_json::to_vec_pretty(&baseline)?)?;
        println!("{}", json!({"ok": true, "descriptor": serde_json::to_value(&baseline)?}));
        return Ok(());
    }
    let descriptor = arguments.get(1).ok_or("expected descriptor")?;
    let serialized = fs::read(descriptor)?;
    let baseline: ScopedBaseline = serde_json::from_slice(&serialized)?;
    let value: Value = serde_json::from_slice(&serialized)?;
    let root = PathBuf::from(value["root"].as_str().ok_or("descriptor root")?);
    match operation.as_str() {
        "enroll" => {
            let path = arguments.get(2).ok_or("expected path")?;
            baseline.enroll(path)?;
            println!("{}", json!({"ok": true, "path": path}));
        }
        "inspect" => {
            let paths = baseline.paths()?;
            let changed = baseline.changed_paths(&root)?;
            let diff = baseline.diff(&root)?;
            println!("{}", json!({"ok": true, "paths": paths, "changed_paths": changed, "diff": diff}));
        }
        "serialize" => {
            let output = arguments.get(2).ok_or("expected new descriptor path")?;
            fs::write(output, serde_json::to_vec_pretty(&baseline)?)?;
            println!("{}", json!({"ok": true}));
        }
        _ => return Err("unknown operation".into()),
    }
    Ok(())
}
