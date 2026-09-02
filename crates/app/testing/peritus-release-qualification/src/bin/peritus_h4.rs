//! Native H4 evidence-envelope preparation and admission operator.

fn main() {
    if let Err(error) = peritus_release_qualification::operator::run_from_env() {
        eprintln!("peritus-h4: {error}");
        std::process::exit(1);
    }
}
