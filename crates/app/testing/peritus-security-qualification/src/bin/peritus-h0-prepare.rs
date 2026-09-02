//! Thin native H0 candidate-preparation entry point.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    peritus_security_qualification::run_h0_preparation()
}
