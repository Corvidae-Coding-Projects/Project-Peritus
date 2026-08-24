//! Observable behaviors exposed by the process test fixture.

use std::{
    env,
    io::{Read, Write},
    process::Command,
    thread,
    time::Duration,
};

pub fn run() {
    let arguments: Vec<String> = env::args().collect();
    match arguments.get(1).map(String::as_str) {
        Some("literal") => literal(&arguments[2..]),
        Some("pipe") => pipe(),
        Some("pty") => pty(),
        Some("output") => output(),
        Some("dual-output") => dual_output(),
        Some("control") => control(),
        Some("tree") => tree(arguments.get(2)),
        Some("tree-child") => tree_child(arguments.get(2)),
        Some("pipe-holder") => pipe_holder(),
        Some("hold-open") => hold_open(),
        _ => {}
    }
}

fn literal(arguments: &[String]) {
    let cwd = env::current_dir().expect("fixture current directory");
    let alpha = env::var("PERITUS_ALPHA").expect("fixture alpha environment");
    let beta = env::var("PERITUS_BETA").expect("fixture beta environment");
    let mut output = std::io::stdout().lock();
    write_field(&mut output, cwd.to_string_lossy().as_bytes());
    write_field(&mut output, alpha.as_bytes());
    write_field(&mut output, beta.as_bytes());
    for argument in arguments {
        write_field(&mut output, argument.as_bytes());
    }
}

fn write_field(output: &mut impl Write, value: &[u8]) {
    output.write_all(value).expect("fixture field");
    output.write_all(&[0]).expect("fixture delimiter");
}

fn pipe() {
    let mut input = [0_u8; 10];
    std::io::stdin().read_exact(&mut input).expect("fixture pipe input");
    assert_eq!(&input, b"pipe-input");
    std::io::stdout().write_all(b"pipe-out").expect("fixture stdout");
    std::io::stderr().write_all(b"pipe-err").expect("fixture stderr");
}

#[cfg(unix)]
fn pty() {
    configure_raw_terminal();
    let mut input = [0_u8; 9];
    std::io::stdin().read_exact(&mut input).expect("fixture PTY input");
    assert_eq!(&input, b"pty-input");
    std::io::stdout().write_all(&input).expect("fixture PTY output");
}

#[cfg(not(unix))]
fn pty() {
    panic!("the process fixture requires a Unix PTY");
}

#[cfg(unix)]
fn configure_raw_terminal() {
    use nix::sys::termios::{SetArg, cfmakeraw, tcgetattr, tcsetattr};

    let input = std::io::stdin();
    let mut terminal = tcgetattr(&input).expect("fixture terminal attributes");
    cfmakeraw(&mut terminal);
    tcsetattr(&input, SetArg::TCSANOW, &terminal).expect("fixture raw terminal");
}

fn output() {
    let mut output = std::io::stdout().lock();
    output.write_all(b"abcdefgh").expect("fixture bounded output");
    output.flush().expect("flush bounded output");
    drop(output);
    thread::sleep(Duration::from_secs(5));
}

fn dual_output() {
    std::io::stdout().write_all(b"artifact-out").expect("fixture artifact stdout");
    std::io::stderr().write_all(b"artifact-err").expect("fixture artifact stderr");
}

fn control() {
    loop {
        thread::sleep(Duration::from_millis(50));
    }
}

fn tree(depth: Option<&String>) {
    let depth = depth.and_then(|value| value.parse::<u64>().ok()).unwrap_or(2);
    let executable = env::current_exe().expect("fixture executable");
    let mut child = Command::new(executable)
        .arg("tree-child")
        .arg(depth.to_string())
        .spawn()
        .expect("fixture descendant");
    child.wait().expect("fixture descendant wait");
}

fn tree_child(depth: Option<&String>) {
    let depth = depth.and_then(|value| value.parse::<u64>().ok()).unwrap_or(0);
    if depth > 1 {
        tree(Some(&(depth - 1).to_string()));
    } else {
        thread::sleep(Duration::from_millis(200));
    }
}

fn pipe_holder() {
    let mut holder = Command::new(env::current_exe().expect("fixture executable"))
        .arg("hold-open")
        .spawn()
        .expect("fixture pipe holder");
    let _reaper = thread::spawn(move || holder.wait());
}

fn hold_open() {
    thread::sleep(Duration::from_secs(5));
}
