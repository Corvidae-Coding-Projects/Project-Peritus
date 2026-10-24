//! Product-run adapter for the shared credential-free preparation implementation.

pub(super) use peritus_modules::preparation::{Completed, Failure, Specification, fetch};

pub(super) fn validate(specification: &Specification, storage_bytes: u64) -> Result<(), String> {
    if specification.maximum_bytes > 64 * 1024 * 1024 {
        return Err("maximum_bytes exceeds the module-job preparation ceiling of 64 MiB".into());
    }
    peritus_modules::preparation::validate(specification, storage_bytes).map(|_| ())
}
