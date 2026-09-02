//! Reviewed native H2 package and host qualification controller.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    peritus_platform_qualification::run_h2_native_controller()
}
