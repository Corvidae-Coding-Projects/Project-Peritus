//! A normal executable consumer of Peritus's public command API; no test harness.
use peritus_process::{OutputStream, ProcessStore};
use peritus_product_runner::{CommandRuntime, PreviewCommand, PreviewProcessState};
use peritus_types::RunId;
use std::{
    error::Error,
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

fn main() -> Result<(), Box<dyn Error>> {
    let root = PathBuf::from(std::env::args().nth(1).ok_or("expected disposable root")?);
    let workspace = root.join("workspace");
    fs::create_dir_all(&workspace)?;
    let process_root = root.join("processes");
    let runtime_root = root.join("commands");
    let run = RunId::new([17; 16]).map_err(|error| format!("{error:?}"))?;
    let processes = ProcessStore::open(&process_root, &workspace)?;
    let runtime = CommandRuntime::open(runtime_root.clone(), workspace.clone(), run, processes)?;
    let sizes = [0usize, 1, 16_383, 16_384, 16_385, 8 * 1024 * 1024 + 17, 37_349_240];
    let mut rows = Vec::new();
    for (case, size) in sizes.into_iter().enumerate() {
        // Input bytes are specified independently of command capture: all byte values, including
        // NUL and invalid UTF-8, recur at a fixed 256-byte period.
        let expected: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();
        let input = workspace.join(format!("case-{case}.bin"));
        fs::write(&input, &expected)?;
        let command = PreviewCommand::new(
            "/usr/bin/cat".into(),
            vec![input.to_string_lossy().into()],
            workspace.clone(),
            Duration::from_secs(30),
            false,
            24,
            80,
            format!("capture-case-{case}"),
            vec![],
        )?;
        let mut owner = None;
        let launch = runtime.launch_preview_registered(&command, &mut |value| {
            owner = Some(value);
            Ok(())
        })?;
        let started = Instant::now();
        let observation = loop {
            let value = runtime.observe_preview(&launch)?;
            if value.state() != PreviewProcessState::Running {
                break value;
            }
            if started.elapsed() > Duration::from_secs(35) {
                return Err("command did not finish".into());
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        if observation.state() != PreviewProcessState::Succeeded {
            return Err(format!(
                "case {case}: command failed: {:?}; {:?}",
                observation.state(),
                observation.progress()
            )
            .into());
        }
        if observation.stdout().len() > 70_000 {
            return Err("large output was expanded into the model-facing preview".into());
        }
        let owner = owner.ok_or("command registration missing")?;
        let mut pages = 0;
        for offset in [0, size / 2, size.saturating_sub(19), size] {
            let page =
                runtime.retained_preview_range(owner, OutputStream::Stdout, offset as u64, 37)?;
            if page.total_bytes() != size as u64
                || page.bytes() != &expected[offset..(offset + 37).min(size)]
            {
                return Err(format!("case {case}: incorrect exact spool page at {offset}").into());
            }
            pages += 1;
        }
        if runtime
            .retained_preview_range(owner, OutputStream::Stdout, (size + 1) as u64, 37)
            .is_ok()
        {
            return Err("past-EOF read was accepted".into());
        }
        // Finalized artifact retrieval proves publication, separately from successful process exit.
        if size > 0 {
            let page = runtime.preview_output_range(
                launch.process_id(),
                OutputStream::Stdout,
                (size / 2) as u64,
                53,
            )?;
            if page.digest().is_none()
                || page.total_bytes() != size as u64
                || page.bytes() != &expected[size / 2..(size / 2 + 53).min(size)]
            {
                return Err("finalized artifact differs from source bytes".into());
            }
        }
        let reopened = CommandRuntime::open(
            runtime_root.clone(),
            workspace.clone(),
            run,
            ProcessStore::open(&process_root, &workspace)?,
        )?;
        let _ = reopened.reconnect_preview(owner)?;
        let page = reopened.retained_preview_range(
            owner,
            OutputStream::Stdout,
            size.saturating_sub(17) as u64,
            31,
        )?;
        if page.bytes() != &expected[size.saturating_sub(17)..] {
            return Err("reopened runtime changed retained output".into());
        }
        rows.push(format!("{{\"case\":{case},\"bytes\":{size},\"pages\":{pages},\"prefix_bytes\":{},\"published\":true,\"reopened\":true,\"status\":\"PASS\"}}",observation.stdout().len()));
    }
    fs::write(root.join("results.json"), format!("[{}]\n", rows.join(",")))?;
    println!(
        "PASS actual command API: {} sizes, binary byte pages, EOF rejection, finalized artifacts and reopened ownership",
        rows.len()
    );
    Ok(())
}
