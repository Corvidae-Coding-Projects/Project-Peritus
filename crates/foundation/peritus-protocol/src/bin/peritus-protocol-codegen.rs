//! Reproducible protocol schema and compatibility-corpus generator.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    peritus_protocol::schema::run_codegen(std::env::args_os().skip(1))
}
