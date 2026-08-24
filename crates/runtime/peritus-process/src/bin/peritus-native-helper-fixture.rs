//! Native-helper protocol fixture binary used by process integration tests.

mod native_helper_fixture;

fn main() {
    if native_helper_fixture::run().is_err() {
        std::process::exit(125);
    }
}
