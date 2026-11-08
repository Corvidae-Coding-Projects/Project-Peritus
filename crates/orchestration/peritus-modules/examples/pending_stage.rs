//! Exercise manifest-bound stage reads, cancellation, publication, and restart on real files.
use peritus_modules::{Definition, Job};
use std::{error::Error, fs, io};

fn main() -> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    let package = temp.path().join("package");
    fs::create_dir(&package)?;
    fs::write(
        package.join("module.toml"),
        r#"schema = 1
id = "pending-stage-replay"
version = "1.0.0"
name = "Pending stage replay"
description = "Real isolated job lifecycle"
[[roles]]
id = "research"
model = "writer"
instructions = "Collect the initial observation."
tools = []
[[roles]]
id = "check"
model = "reviewer"
instructions = "Review only declared inputs."
tools = []
[[stages]]
id = "first"
role = "research"
inputs = []
[[stages]]
id = "middle"
role = "research"
inputs = []
[[stages]]
id = "final"
role = "check"
inputs = ["first"]
"#,
    )?;
    let definition = Definition::from_directory(&package)?;
    let root = temp.path().join("jobs");
    let mut job = Job::open(&root, definition.clone(), 7, "exact task")?;
    let namespace = {
        let stage = job.pending_stage()?.ok_or_else(|| io::Error::other("missing first stage"))?;
        assert_eq!(stage.spec().id, "first");
        assert_eq!(stage.role().id, "research");
        assert_eq!(stage.definition().digest, definition.digest);
        assert_eq!(stage.task(), "exact task");
        assert_eq!(stage.input_revision(), 7);
        assert!(stage.read_input("first", 0, 64).is_err());
        assert!(Job::open(&root, definition.clone(), 7, "exact task").is_err());
        stage.namespace().to_owned()
        // Dropping a stage without completing it preserves the original checkpoint.
    };
    assert!(job.state().completed.is_empty());
    assert!(job.pending_stage()?.unwrap().complete("").is_err());
    assert!(job.state().completed.is_empty());
    job.pending_stage()?.unwrap().complete("🙂α first\n")?;
    drop(job);
    let mut job = Job::open(&root, definition.clone(), 7, "exact task")?;
    {
        let stage = job.pending_stage()?.unwrap();
        assert_eq!(stage.namespace(), namespace);
        assert_eq!(stage.spec().id, "middle");
        assert!(
            stage.read_input("first", 0, 64).is_err(),
            "a committed output still requires a declared dependency"
        );
        stage.complete("middle output")?;
    }
    let checkpoint = job.root().join("job.json");
    let before = fs::read(&checkpoint)?;
    {
        let stage = job.pending_stage()?.unwrap();
        assert_eq!(stage.role().model, "reviewer");
        assert_eq!(stage.spec().id, "final");
        assert!(stage.read_input("middle", 0, 64).is_err());
        let page = stage.read_input("first", 0, 4)?;
        assert_eq!(page["text"], "🙂");
        assert_eq!(page["next_offset"], 4);
        assert!(stage.read_input("first", 1, 4).is_err());
        assert_eq!(fs::read(&checkpoint)?, before);
        let page = stage.read_input("first", 4, 64)?;
        assert_eq!(page["text"], "α first\n");
        assert!(page["next_offset"].is_null());
        stage.complete("final output")?;
    }
    assert!(job.pending_stage()?.is_none());
    assert_eq!(job.result()?.as_deref(), Some("final output"));
    drop(job);
    let mut reopened = Job::open(&root, definition, 7, "exact task")?;
    assert_eq!(reopened.state().completed.len(), 3);
    assert!(reopened.pending_stage()?.is_none());
    println!(
        "PASS pending-stage role/input binding, exclusive ownership, cancellation, UTF-8 paging, durable publication, and restart"
    );
    Ok(())
}
