//! Fail-closed reference-machine qualification.

use peritus_benchmarks::{QualificationError, ReferenceMachine, StableId};
use serde::Serialize;

/// Measured host facts retained beside one H3 campaign.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MachineObservation {
    measured: ReferenceMachine,
    raw: RawMachineFacts,
}

/// Unnormalized operating-system facts retained beside a normalized machine class.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RawMachineFacts {
    cpu_model: String,
    memory_bytes: u64,
}

impl RawMachineFacts {
    /// Validates raw probe facts before they become evidence.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] for empty or oversized CPU text or zero memory.
    pub fn new(
        cpu_model: impl Into<String>,
        memory_bytes: u64,
    ) -> Result<Self, QualificationError> {
        let cpu_model = cpu_model.into();
        if cpu_model.trim().is_empty() || cpu_model.len() > 512 || memory_bytes == 0 {
            return Err(QualificationError::InvalidValue {
                field: "machine_probe.raw_facts",
                reason: "raw CPU text must contain 1 through 512 bytes and memory must be nonzero",
            });
        }
        Ok(Self { cpu_model, memory_bytes })
    }

    /// Returns the exact CPU text reported by the operating system.
    #[must_use]
    pub fn cpu_model(&self) -> &str {
        &self.cpu_model
    }

    /// Returns the byte count reported by the operating system.
    #[must_use]
    pub const fn memory_bytes(&self) -> u64 {
        self.memory_bytes
    }
}

impl MachineObservation {
    /// Constructs a complete measured-host observation.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when CPU text is empty or overlong, or when measured cores
    /// or memory are zero.
    pub fn new(
        operating_system: StableId,
        architecture: StableId,
        cpu_model: impl Into<String>,
        logical_cores: u16,
        memory_bytes: u64,
        storage_class: StableId,
    ) -> Result<Self, QualificationError> {
        let cpu_model = cpu_model.into();
        ReferenceMachine::new(
            operating_system,
            architecture,
            cpu_model.clone(),
            logical_cores,
            memory_bytes,
            storage_class,
        )
        .and_then(|measured| {
            RawMachineFacts::new(cpu_model, memory_bytes).map(|raw| Self { measured, raw })
        })
    }

    /// Constructs an observation whose normalized machine class differs from its retained raw
    /// hardware facts.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when either CPU string or the normalized capacity is invalid.
    pub fn from_probe(
        operating_system: StableId,
        architecture: StableId,
        normalized_cpu_model: impl Into<String>,
        logical_cores: u16,
        normalized_memory_bytes: u64,
        storage_class: StableId,
        raw: RawMachineFacts,
    ) -> Result<Self, QualificationError> {
        ReferenceMachine::new(
            operating_system,
            architecture,
            normalized_cpu_model,
            logical_cores,
            normalized_memory_bytes,
            storage_class,
        )
        .map(|measured| Self { measured, raw })
    }

    /// Compares every measured fact with the reviewed profile contract.
    #[must_use]
    pub fn assess(&self, expected: &ReferenceMachine) -> MachineAssessment {
        let mut mismatches = Vec::new();
        retain(
            &mut mismatches,
            self.measured.operating_system() == expected.operating_system(),
            MachineMismatch::OperatingSystem,
        );
        retain(
            &mut mismatches,
            self.measured.architecture() == expected.architecture(),
            MachineMismatch::Architecture,
        );
        retain(
            &mut mismatches,
            self.measured.cpu_model() == expected.cpu_model(),
            MachineMismatch::CpuModel,
        );
        retain(
            &mut mismatches,
            self.measured.logical_cores() == expected.logical_cores(),
            MachineMismatch::LogicalCores,
        );
        retain(
            &mut mismatches,
            self.measured.memory_bytes() == expected.memory_bytes(),
            MachineMismatch::MemoryBytes,
        );
        retain(
            &mut mismatches,
            self.measured.storage_class() == expected.storage_class(),
            MachineMismatch::StorageClass,
        );
        MachineAssessment { matches: mismatches.is_empty(), mismatches }
    }

    /// Returns the exact measured-host facts retained by this observation.
    #[must_use]
    pub const fn reference_machine(&self) -> &ReferenceMachine {
        &self.measured
    }

    /// Returns the unnormalized CPU text reported by the operating system.
    #[must_use]
    pub fn raw_cpu_model(&self) -> &str {
        self.raw.cpu_model()
    }

    /// Returns the usable or installed byte count reported by the operating system.
    #[must_use]
    pub const fn raw_memory_bytes(&self) -> u64 {
        self.raw.memory_bytes()
    }
}

/// Reference-machine field that differs from the measured host.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MachineMismatch {
    /// Operating-system family differs.
    OperatingSystem,
    /// Architecture differs.
    Architecture,
    /// CPU model differs.
    CpuModel,
    /// Logical-core count differs.
    LogicalCores,
    /// Installed memory differs.
    MemoryBytes,
    /// Storage class differs.
    StorageClass,
}

/// Exact comparison result retained before production workloads start.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MachineAssessment {
    matches: bool,
    mismatches: Vec<MachineMismatch>,
}

impl MachineAssessment {
    /// Returns whether every reviewed machine field matched exactly.
    #[must_use]
    pub const fn matches(&self) -> bool {
        self.matches
    }

    /// Returns mismatched fields in stable contract order.
    #[must_use]
    pub fn mismatches(&self) -> &[MachineMismatch] {
        &self.mismatches
    }
}

fn retain(mismatches: &mut Vec<MachineMismatch>, matches: bool, mismatch: MachineMismatch) {
    if !matches {
        mismatches.push(mismatch);
    }
}

#[cfg(test)]
mod tests {
    use peritus_benchmarks::{ReferenceMachine, StableId};

    use super::{MachineMismatch, MachineObservation};

    #[test]
    fn exact_observation_matches_reference() {
        let reference = reference();
        let observed = observation("cpu", 32, 64);
        assert!(observed.assess(&reference).matches());
    }

    #[test]
    fn mismatch_order_is_stable_and_complete() {
        let assessment = observation("different", 24, 32).assess(&reference());
        assert!(!assessment.matches());
        assert_eq!(
            assessment.mismatches(),
            &[
                MachineMismatch::CpuModel,
                MachineMismatch::LogicalCores,
                MachineMismatch::MemoryBytes
            ]
        );
    }

    fn reference() -> ReferenceMachine {
        ReferenceMachine::new(id("linux"), id("x86_64"), "cpu", 32, 64, id("nvme-gen4"))
            .expect("reference")
    }

    fn observation(cpu: &str, cores: u16, memory: u64) -> MachineObservation {
        MachineObservation::new(id("linux"), id("x86_64"), cpu, cores, memory, id("nvme-gen4"))
            .expect("observation")
    }

    fn id(value: &str) -> StableId {
        StableId::new(value).expect("stable id")
    }
}
