//! Thin host-native Peritus package assembler entry point.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    peritus_platform_qualification::run_package_builder()
}
