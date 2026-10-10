//! Exact non-secret target environment projection.

use core::fmt;

use peritus_process::ExecutionPlan;

use crate::{
    MacosError, MacosOperation,
    canonical::{Reader, Writer},
    error,
};

/// One exact non-secret environment assignment carried in the protected manifest.
#[derive(Clone, Eq, Ord, PartialEq, PartialOrd)]
pub struct EnvironmentEntry {
    name: String,
    value: String,
}

impl EnvironmentEntry {
    /// Creates one portable target assignment.
    ///
    /// # Errors
    /// Rejects an empty name, `=` or NUL in the name, or NUL in the value.
    pub fn new(name: String, value: String) -> Result<Self, MacosError> {
        if name.is_empty() || name.contains(['=', '\0']) || value.as_bytes().contains(&0) {
            return Err(error::invalid(
                MacosOperation::Manifest,
                "target environment assignment contains an invalid native name or NUL",
            ));
        }
        Ok(Self { name, value })
    }

    /// Returns the exact variable name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the exact non-secret variable value.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }

    pub(crate) fn encode(&self, writer: &mut Writer) -> Result<(), MacosError> {
        writer.string(&self.name)?;
        writer.string(&self.value)
    }

    pub(crate) fn decode(reader: &mut Reader<'_>) -> Result<Self, MacosError> {
        Self::new(reader.string()?, reader.string()?)
    }
}

impl fmt::Debug for EnvironmentEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EnvironmentEntry")
            .field("name", &self.name)
            .field("value_bytes", &self.value.len())
            .finish()
    }
}

pub(crate) fn project_environment(
    execution: &ExecutionPlan,
) -> Result<Vec<EnvironmentEntry>, MacosError> {
    let mut entries = execution
        .environment()
        .variables()
        .iter()
        .map(|variable| {
            EnvironmentEntry::new(variable.name().to_owned(), variable.value().to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    canonicalize(&mut entries)?;
    Ok(entries)
}

pub(crate) fn canonicalize(entries: &mut [EnvironmentEntry]) -> Result<(), MacosError> {
    entries.sort_by(|left, right| left.name.cmp(&right.name));
    if entries.windows(2).any(|pair| pair[0].name == pair[1].name) {
        return Err(error::invalid(
            MacosOperation::Manifest,
            "target environment contains duplicate names",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_projection_has_no_host_name_count_or_value_ceiling() {
        let mut entries = (0..1_025)
            .map(|index| {
                EnvironmentEntry::new(
                    format!("VARIABLE_{index}_{}", "N".repeat(256)),
                    if index == 0 { "v".repeat(65 * 1_024) } else { String::new() },
                )
            })
            .collect::<Result<Vec<_>, _>>()
            .expect("environment projection");
        canonicalize(&mut entries).expect("canonical environment");
        assert_eq!(entries.len(), 1_025);
        assert_eq!(entries[0].value().len(), 65 * 1_024);
    }
}
