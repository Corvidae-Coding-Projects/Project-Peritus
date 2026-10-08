//! Recover prior native usage under the same exclusive session owner as the next call.

use std::{
    fs::{self, File},
    io::{Read as _, Seek as _, SeekFrom},
    path::Path,
};

use peritus_model_protocol::UsageCounters;
use peritus_provider_core::ProviderCoreError;

use super::super::super::output::usage::{decode, high_water};

const JOURNAL_TAIL_BYTES: u64 = 64 * 1024;

pub(super) fn recover(root: Option<&Path>) -> Result<UsageCounters, ProviderCoreError> {
    let Some(root) = root else { return Ok(UsageCounters::default()) };
    let mut usage = UsageCounters::default();
    for entry in fs::read_dir(root).map_err(|_| error("cannot inspect native usage journals"))? {
        let entry = entry.map_err(|_| error("cannot inspect native usage journal"))?;
        if !entry.file_type().map_err(|_| error("cannot inspect native usage turn"))?.is_dir() {
            continue;
        }
        let file = match File::open(entry.path().join("native.jsonl")) {
            Ok(file) => file,
            Err(failure) if failure.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err(error("cannot read native usage journal")),
        };
        usage = high_water(usage, completed_usage(file)?);
    }
    Ok(usage)
}

fn completed_usage(mut file: File) -> Result<UsageCounters, ProviderCoreError> {
    let length = file.metadata().map_err(|_| error("cannot inspect native usage length"))?.len();
    let start = length.saturating_sub(JOURNAL_TAIL_BYTES);
    // Include the preceding byte so an exact line boundary is not mistaken for a partial line.
    file.seek(SeekFrom::Start(start.saturating_sub(1)))
        .map_err(|_| error("cannot seek native usage journal"))?;
    let mut bytes = Vec::new();
    file.take(JOURNAL_TAIL_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| error("cannot read native usage journal tail"))?;
    let tail = if start == 0 {
        bytes.as_slice()
    } else {
        bytes.iter().position(|byte| *byte == b'\n').map_or(&[][..], |index| &bytes[index + 1..])
    };
    let mut usage = UsageCounters::default();
    for line in tail.split_inclusive(|byte| *byte == b'\n') {
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let event: serde_json::Value = match serde_json::from_slice(line) {
            Ok(event) => event,
            Err(failure) if !line.ends_with(b"\n") && failure.is_eof() => break,
            Err(_) => return Err(error("retained native usage journal is malformed")),
        };
        if event.get("type").and_then(serde_json::Value::as_str) == Some("turn.completed") {
            let next = decode(event.get("usage"))
                .map_err(|_| error("retained native usage counters are invalid"))?;
            usage = high_water(usage, next);
        }
    }
    Ok(usage)
}

const fn error(detail: &'static str) -> ProviderCoreError {
    ProviderCoreError::configuration("codex_runtime_usage", detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reopening_recovers_high_water_independent_of_directory_order_and_torn_tail() {
        let root = tempfile::tempdir().unwrap();
        for (name, usage) in [("turn-z", 52_438), ("turn-a", 18_744)] {
            let turn = root.path().join(name);
            fs::create_dir(&turn).unwrap();
            fs::write(turn.join("native.jsonl"), format!("{{\"type\":\"turn.completed\",\"usage\":{{\"input_tokens\":{usage}}}}}\n{{\"partial\":" )).unwrap();
        }
        assert_eq!(recover(Some(root.path())).unwrap().input_tokens(), Some(52_438));
        assert_eq!(recover(None).unwrap(), UsageCounters::default());
    }

    #[test]
    fn large_message_tail_preserves_the_small_completed_usage_record() {
        let root = tempfile::tempdir().unwrap();
        let turn = root.path().join("turn-large");
        fs::create_dir(&turn).unwrap();
        let message =
            format!("{{\"type\":\"item.completed\",\"text\":\"{}\"}}\n", "x".repeat(128 * 1024));
        fs::write(turn.join("native.jsonl"), message + "{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":123,\"output_tokens\":4}}\n").unwrap();
        let usage = recover(Some(root.path())).unwrap();
        assert_eq!(usage.input_tokens(), Some(123));
        assert_eq!(usage.output_tokens(), Some(4));
    }

    #[test]
    fn invalid_persisted_counter_cannot_reset_the_accounting_baseline() {
        let root = tempfile::tempdir().unwrap();
        let turn = root.path().join("turn-invalid");
        fs::create_dir(&turn).unwrap();
        fs::write(
            turn.join("native.jsonl"),
            b"{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":-1}}\n",
        )
        .unwrap();
        assert!(recover(Some(root.path())).is_err());
    }

    #[test]
    fn complete_final_usage_without_a_newline_remains_accounted() {
        let root = tempfile::tempdir().unwrap();
        let turn = root.path().join("turn-no-newline");
        fs::create_dir(&turn).unwrap();
        fs::write(
            turn.join("native.jsonl"),
            b"\n{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":42}}",
        )
        .unwrap();
        assert_eq!(recover(Some(root.path())).unwrap().input_tokens(), Some(42));
    }
}
