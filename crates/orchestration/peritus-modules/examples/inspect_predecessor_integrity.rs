//! Replay a damaged predecessor hidden behind an intact final module output.
//!
//! Run with `cargo run --locked -p peritus-modules --example inspect_predecessor_integrity`.
use std::{error::Error, fs, io};

use peritus_modules::{Definition, Job};

fn main() -> Result<(), Box<dyn Error>> {
    let temporary = tempfile::tempdir()?;
    let package = temporary.path().join("package");
    fs::create_dir(&package)?;
    fs::write(
        package.join("module.toml"),
        r#"schema = 1
id = "inspection-regression"
version = "1.0.0"
name = "Inspection regression"
description = "Disposable checkpoint integrity fixture"
resources = []
[[roles]]
id = "writer"
model = "writer"
instructions = "Use the provided stage input."
tools = []
[[stages]]
id = "first"
role = "writer"
inputs = []
[[stages]]
id = "final"
role = "writer"
inputs = ["first"]
"#,
    )?;
    let definition = Definition::from_directory(&package)?;
    let root = temporary.path().join("jobs");
    let mut job = Job::open(&root, definition.clone(), 1, "inspect predecessors")?;
    job.complete("first", "first output")?;
    job.complete("final", "final output")?;
    // Inspection is intentionally non-owning and must also work while the writer holds the lock.
    let baseline = Job::inspect(&root, definition.clone(), 1, "inspect predecessors", 0)?;
    let first = &job.state().completed[0];
    let artifact = job.root().join(format!("{}-{}.txt", first.stage, first.sha256));
    let checkpoint = job.root().join("job.json");
    let before = fs::read(&checkpoint)?;
    fs::write(&artifact, "wrong output")?; // equal length; a size-only check cannot detect it
    if Job::inspect(&root, definition.clone(), 1, "inspect predecessors", 0).is_ok() {
        return Err(
            io::Error::other("inspection accepted a corrupted completed predecessor").into()
        );
    }
    if fs::read(&checkpoint)? != before {
        return Err(io::Error::other("failed inspection modified the checkpoint").into());
    }
    fs::write(&artifact, "first output")?;
    if Job::inspect(&root, definition.clone(), 1, "inspect predecessors", 0)? != baseline {
        return Err(io::Error::other("restored bytes did not restore the exact inspection").into());
    }
    drop(job);
    let reopened = Job::open(&root, definition, 1, "inspect predecessors")?;
    if reopened.result()?.as_deref() != Some("final output") {
        return Err(io::Error::other("reopen changed the final output").into());
    }
    println!("module inspection rejects damaged predecessors and recovers after restoration");
    Ok(())
}
