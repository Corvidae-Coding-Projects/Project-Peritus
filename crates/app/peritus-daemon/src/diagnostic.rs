//! Best-effort diagnostics that must never turn recovery into a startup failure.

pub fn report(message: &str) {
    let mut stderr = std::io::stderr().lock();
    let _ = std::io::Write::write_all(&mut stderr, message.as_bytes())
        .and_then(|()| std::io::Write::write_all(&mut stderr, b"\n"));
}
