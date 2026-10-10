# FINDING-0020: The macOS updater repair failed the repository's target lint policy

The first macOS cleanup repair retained two redundant `std::mem::` prefixes and failed the workspace's deny-level `unused_qualifications` lint on the Apple target. The final bytes remove only those prefixes; FFI sizes, conversions, safety conditions, and cleanup behavior are unchanged. The exact-lint Apple cross-check and integrated policy pass.
