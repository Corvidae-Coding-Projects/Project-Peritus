//! Stable-toolchain replay of every regular input in one bounded corpus directory.

use peritus_bug_discovery::{DiscoveryTarget, MAX_INPUT_BYTES, check_input};
use std::io::Read as _;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err(
            "usage: discovery-replay <sse|ndjson|working_state|provider_sequence> <corpus-directory>"
                .into(),
        );
    }
    let target =
        args[0].to_str().and_then(DiscoveryTarget::parse).ok_or("unknown discovery target")?;
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(&args[1])? {
        if entries.len() >= 4096 {
            return Err("corpus exceeds 4096 entries".into());
        }
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            return Err("corpus contains non-regular input".into());
        }
        entries.push(entry.path());
    }
    entries.sort();
    if entries.is_empty() {
        return Err("corpus contains no inputs".into());
    }
    for path in &entries {
        let mut bytes = Vec::new();
        std::fs::File::open(path)?.take((MAX_INPUT_BYTES + 1) as u64).read_to_end(&mut bytes)?;
        if bytes.len() > MAX_INPUT_BYTES {
            return Err(format!("oversized corpus input: {}", path.display()).into());
        }
        check_input(target, &bytes);
    }
    println!("replayed {} inputs for {target:?}", entries.len());
    Ok(())
}
