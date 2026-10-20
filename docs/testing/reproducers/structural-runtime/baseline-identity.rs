//! Stable candidate identity from the pre-remediation production library.
fn main() {
    let path = std::env::args_os().nth(1).expect("workspace");
    println!(
        "{:?}",
        peritus_product_runner::ProductRunner::candidate_digest(std::path::Path::new(&path))
            .expect("candidate identity")
    );
}
