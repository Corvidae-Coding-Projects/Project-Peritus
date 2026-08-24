//! Observable target behavior for C4 shell and quality integration tests.

use std::io::{Read, Write};
use std::time::Duration;

pub fn run() {
    let arguments: Vec<String> = std::env::args().collect();
    match arguments.get(1).map(String::as_str) {
        Some("shell") => shell(arguments.get(2).map_or("missing", String::as_str)),
        Some("quality-valid") => quality(arguments.get(2), true),
        Some("quality-invalid") => quality(arguments.get(2), false),
        Some("control") => loop {
            std::thread::sleep(Duration::from_millis(20));
        },
        _ => std::process::exit(2),
    }
}

fn quality(marker: Option<&String>, valid: bool) {
    let marker = marker.expect("quality marker argument");
    let contents = std::fs::read_to_string(marker).expect("quality project marker");
    assert_eq!(contents, "project-under-test");
    let mut stdout = std::io::stdout().lock();
    if valid {
        write!(stdout, r#"{{"passed":true}}"#).expect("quality stdout");
    } else {
        write!(stdout, "not-json").expect("quality stdout");
    }
    stdout.flush().expect("quality stdout flush");
}

fn shell(argument: &str) {
    let mut input = [0_u8; 5];
    std::io::stdin().read_exact(&mut input).expect("five stdin bytes");
    assert_eq!(&input, b"hello");
    let mut stdout = std::io::stdout().lock();
    write!(stdout, "argv:{argument}:hello").expect("stdout");
    stdout.flush().expect("stdout flush");
    std::io::stderr().write_all(b"shell-stderr").expect("stderr");
}
