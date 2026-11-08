fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() == Some("qualify-dependency-child") {
        println!("dependency-child");
        std::process::exit(17);
    }
    let fixture = tempfile::tempdir()?;
    let workspace = fixture.path().join("plain-folder");
    let state = fixture.path().join("state");
    std::fs::create_dir_all(&state)?;
    let observation = peritus_product_runner::qualification::qualify_tool_process_failure(
        &workspace,
        state.join("effects.bin"),
        &std::env::current_exe()?,
        "delivery-legacy",
        1,
    )?;
    assert_eq!(observation.exit_code(), 17);
    assert!(!workspace.join(".git").exists());
    assert!(observation.receipt_bytes() > 0);
    println!(
        "PASS: real non-Git tool execution retained exit 17, stdout evidence, and durable effect receipt"
    );
    Ok(())
}
