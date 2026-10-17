//! Human-accessible web setup, diagnostics and source operations without a daemon.

use crate::error::CliError;
use serde_json::{Value, json};

pub(crate) fn execute(arguments: &[String]) -> Result<Value, CliError> {
    let usage = || {
        CliError::usage(
            "usage: peritus web <setup [SEARXNG_URL]|status|stop|search QUERY|open URL [--refresh]|read SOURCE [OFFSET] [MAX_BYTES]|find SOURCE QUERY [OFFSET]>",
        )
    };
    let operation = arguments.first().ok_or_else(usage)?;
    let required = |index: usize| arguments.get(index).map(String::as_str).ok_or_else(usage);
    let number = |index: usize, default: u64| -> Result<u64, CliError> {
        arguments.get(index).map_or(Ok(default), |v| v.parse().map_err(|_| usage()))
    };
    let maximum = match operation.as_str() {
        "setup" => 2,
        "status" | "stop" => 1,
        "search" => 2,
        "open" => 3,
        "read" | "find" => 4,
        _ => return Err(usage()),
    };
    if arguments.len() > maximum {
        return Err(usage());
    }
    let result = match operation.as_str() {
        "setup" => {
            if let Some(url) = arguments.get(1) {
                peritus_tools_web::configure(url)
            } else {
                peritus_tools_web::setup_local()
            }
        }
        "status" => peritus_tools_web::status(),
        "stop" => peritus_tools_web::stop(),
        name => {
            let store = peritus_tools_web::Store::for_user()
                .map_err(|e| CliError::runtime("open web store", e.to_string()))?;
            match name {
                "search" => store.search(&json!({"query":required(1)?}), &|| false),
                "open" => {
                    if arguments.get(2).is_some_and(|v| v != "--refresh") { return Err(usage()); }
                    store.open(&json!({"url":required(1)?, "refresh":arguments.len() == 3}), &|| false)
                },
                "read" => store.read(&json!({"source":required(1)?, "offset":number(2,0)?, "max_bytes":number(3,32768)?})),
                "find" => store.find(&json!({"source":required(1)?, "query":required(2)?, "offset":number(3,0)?})),
                _ => return Err(usage()),
            }
        }
    };
    result.map_err(|e| CliError::runtime("web operation", e.to_string()))
}
