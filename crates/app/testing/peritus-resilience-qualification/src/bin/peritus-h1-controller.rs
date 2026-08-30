//! Reviewed native H1 release-candidate controller.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    peritus_resilience_qualification::run_h1_native_controller()
}
