fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = std::env::args_os().nth(1).ok_or("missing raw terminal capture")?;
    let mut parser = vt100::Parser::new(40, 160, 0);
    parser.process(&std::fs::read(file)?);
    println!("{}", parser.screen().contents());
    Ok(())
}
