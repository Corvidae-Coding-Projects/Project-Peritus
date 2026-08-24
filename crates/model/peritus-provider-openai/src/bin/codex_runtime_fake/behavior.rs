//! Test-only executable entry behavior.

use std::io::{Read as _, Write as _};
use std::time::Duration;

use super::{contract, scenario, trace};

pub(super) fn run() {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    if !matches!(arguments.first().map(String::as_str), Some("exec" | "login")) {
        return;
    }
    if arguments == ["login", "status"] {
        trace::record("auth");
        if !contract::environment_absent() || trace::executable_name().contains("authentication") {
            std::process::exit(1);
        }
        return;
    }
    let mut stdin = String::new();
    if std::io::stdin().read_to_string(&mut stdin).is_err() || !contract::valid(&arguments, &stdin)
    {
        std::process::exit(90);
    }
    let model = contract::argument_value(&arguments, "--model").unwrap_or_default();
    let invocation = trace::record("turn");
    if model.contains("cancel") {
        trace::record("spin");
        loop {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    let (output, exit) = scenario::output(model, invocation);
    let _ = std::io::stdout().write_all(output.as_bytes());
    let _ = std::io::stdout().flush();
    if exit != 0 {
        std::process::exit(exit);
    }
}
