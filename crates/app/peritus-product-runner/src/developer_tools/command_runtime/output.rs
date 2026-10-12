//! Bounded exact output reads scoped to an existing native command receipt.

use peritus_process::OutputStream;
use serde_json::Value;

use super::{CommandRuntime, DeveloperLoopError, tool};
use crate::developer_tools::wire::{object, required_string};

impl CommandRuntime {
    pub(in crate::developer_tools) fn read_command_output(
        &self,
        arguments: &Value,
    ) -> Result<Value, DeveloperLoopError> {
        let handle = required_string(arguments, "handle")?;
        let stream_name = required_string(arguments, "stream")?;
        let stream = match stream_name {
            "stdout" => OutputStream::Stdout,
            "stderr" => OutputStream::Stderr,
            "terminal" => OutputStream::Terminal,
            _ => return Err(tool("command_output stream must be stdout, stderr, or terminal")),
        };
        let offset = integer(arguments, "offset", 0)?;
        let maximum = integer(arguments, "max_bytes", 16 * 1024)?;
        if !(1..=1024 * 1024).contains(&maximum) {
            return Err(tool("command_output max_bytes must be between 1 and 1048576"));
        }
        let owner = self
            .inner
            .state
            .lock()
            .map_err(|_| tool("command runtime is poisoned"))?
            .recovered_owners
            .get(handle)
            .copied()
            .ok_or_else(|| tool("command_output requires a handle owned by this run"))?;
        if owner.source_run != self.inner.run_id {
            return Err(tool("command_output handle belongs to another run"));
        }
        let maximum = usize::try_from(maximum).map_err(|_| tool("output page size overflowed"))?;
        let (total, bytes) = self
            .inner
            .process_store
            .spooled_stream_range_exact(
                owner.execution_run,
                owner.action,
                owner.process,
                stream,
                offset,
                maximum,
            )
            .map_err(|error| tool(format!("read retained command output: {error}")))?;
        let returned = bytes.len() as u64;
        let next = offset.checked_add(returned).ok_or_else(|| tool("output offset overflowed"))?;
        let (encoding, data) = match std::str::from_utf8(&bytes) {
            Ok(text) => ("utf8", text.to_owned()),
            Err(_) => ("hex", hex(&bytes)),
        };
        Ok(object(vec![
            ("handle", Value::from(handle)),
            ("stream", Value::from(stream_name)),
            ("source", Value::from("retained_process_spool")),
            ("offset", Value::from(offset)),
            ("total_bytes", Value::from(total)),
            ("returned_bytes", Value::from(returned)),
            ("next_offset", if next < total { Value::from(next) } else { Value::Null }),
            ("encoding", Value::from(encoding)),
            ("data", Value::from(data)),
        ]))
    }
}

fn integer(arguments: &Value, name: &str, default: u64) -> Result<u64, DeveloperLoopError> {
    arguments.get(name).map_or(Ok(default), |value| {
        value
            .as_u64()
            .ok_or_else(|| tool(format!("command_output {name} must be an unsigned integer")))
    })
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(char::from(DIGITS[usize::from(byte >> 4)]));
        text.push(char::from(DIGITS[usize::from(byte & 15)]));
    }
    text
}
