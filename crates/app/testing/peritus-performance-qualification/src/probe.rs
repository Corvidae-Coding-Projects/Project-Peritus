//! Current-host observation for exact H3 reference-machine admission.

use std::fs;
#[cfg(target_os = "macos")]
use std::process::Command;

use peritus_benchmarks::StableId;

use crate::{MachineObservation, MachineProbeError, RawMachineFacts};

const GIB: u64 = 1_073_741_824;

/// Reads stable hardware facts without consulting the expected qualification profile.
pub struct MachineProbe;

impl MachineProbe {
    /// Observes this host and retains both raw facts and normalized hardware-class values.
    ///
    /// Storage generation is an operator-supplied reviewed class because common unprivileged host
    /// interfaces do not report it consistently. Every other field is read from the host.
    ///
    /// # Errors
    ///
    /// Returns [`MachineProbeError`] when a required fact is unavailable, malformed, or outside the
    /// stable H3 machine schema.
    pub fn observe(storage_class: StableId) -> Result<MachineObservation, MachineProbeError> {
        let raw_cpu_model = cpu_model()?;
        let raw_memory_bytes = memory_bytes()?;
        let normalized_cpu_model = normalize_cpu_model(&raw_cpu_model);
        let normalized_memory_bytes = normalize_memory_class(raw_memory_bytes)?;
        let logical_cores = u16::try_from(
            std::thread::available_parallelism()
                .map_err(|source| MachineProbeError::io("read logical core count", source))?
                .get(),
        )
        .map_err(|_| MachineProbeError::Invalid("logical core count"))?;
        Ok(MachineObservation::from_probe(
            StableId::new(std::env::consts::OS)?,
            StableId::new(std::env::consts::ARCH)?,
            normalized_cpu_model,
            logical_cores,
            normalized_memory_bytes,
            storage_class,
            RawMachineFacts::new(raw_cpu_model, raw_memory_bytes)?,
        )?)
    }
}

#[cfg(target_os = "linux")]
fn cpu_model() -> Result<String, MachineProbeError> {
    let document = fs::read_to_string("/proc/cpuinfo")
        .map_err(|source| MachineProbeError::io("read /proc/cpuinfo", source))?;
    document
        .lines()
        .find_map(|line| line.split_once(':').filter(|(key, _)| key.trim() == "model name"))
        .map(|(_, value)| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .ok_or(MachineProbeError::Missing("CPU model"))
}

#[cfg(target_os = "linux")]
fn memory_bytes() -> Result<u64, MachineProbeError> {
    let document = fs::read_to_string("/proc/meminfo")
        .map_err(|source| MachineProbeError::io("read /proc/meminfo", source))?;
    let kib = document
        .lines()
        .find_map(|line| line.strip_prefix("MemTotal:"))
        .and_then(|value| value.split_whitespace().next())
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or(MachineProbeError::Missing("memory capacity"))?;
    kib.checked_mul(1024).ok_or(MachineProbeError::Invalid("memory capacity"))
}

#[cfg(target_os = "macos")]
fn cpu_model() -> Result<String, MachineProbeError> {
    sysctl_text("machdep.cpu.brand_string", "CPU model")
}

#[cfg(target_os = "macos")]
fn memory_bytes() -> Result<u64, MachineProbeError> {
    sysctl_text("hw.memsize", "memory capacity")?
        .parse()
        .map_err(|_| MachineProbeError::Invalid("memory capacity"))
}

#[cfg(target_os = "macos")]
fn sysctl_text(key: &'static str, field: &'static str) -> Result<String, MachineProbeError> {
    let output = Command::new("/usr/sbin/sysctl")
        .args(["-n", key])
        .output()
        .map_err(|source| MachineProbeError::io("execute sysctl", source))?;
    if !output.status.success() {
        return Err(MachineProbeError::Missing(field));
    }
    String::from_utf8(output.stdout)
        .map(|value| value.trim().to_owned())
        .map_err(|_| MachineProbeError::Invalid(field))
        .and_then(
            |value| {
                if value.is_empty() { Err(MachineProbeError::Missing(field)) } else { Ok(value) }
            },
        )
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn cpu_model() -> Result<String, MachineProbeError> {
    Err(MachineProbeError::Unsupported)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn memory_bytes() -> Result<u64, MachineProbeError> {
    Err(MachineProbeError::Unsupported)
}

fn normalize_cpu_model(raw: &str) -> String {
    let trimmed = raw.trim();
    let mut words = trimmed.rsplitn(3, ' ');
    let processor = words.next();
    let core_count = words.next();
    let model = words.next();
    if processor == Some("Processor")
        && core_count.is_some_and(|value| {
            value.strip_suffix("-Core").is_some_and(|count| {
                !count.is_empty() && count.bytes().all(|byte| byte.is_ascii_digit())
            })
        })
    {
        model.unwrap_or(trimmed).to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn normalize_memory_class(raw_bytes: u64) -> Result<u64, MachineProbeError> {
    let gib =
        raw_bytes.checked_add(GIB - 1).ok_or(MachineProbeError::Invalid("memory capacity"))? / GIB;
    let class_gib =
        gib.checked_next_power_of_two().ok_or(MachineProbeError::Invalid("memory capacity"))?;
    class_gib.checked_mul(GIB).ok_or(MachineProbeError::Invalid("memory capacity"))
}

#[cfg(test)]
mod tests {
    use super::{GIB, normalize_cpu_model, normalize_memory_class};

    #[test]
    fn amd_core_suffix_is_removed_from_class_name() {
        assert_eq!(normalize_cpu_model("AMD Ryzen 9 7950X 16-Core Processor"), "AMD Ryzen 9 7950X");
        assert_eq!(normalize_cpu_model("Apple M4 Pro"), "Apple M4 Pro");
    }

    #[test]
    fn usable_memory_is_rounded_up_to_the_hardware_class() {
        assert_eq!(normalize_memory_class(62 * GIB).expect("class"), 64 * GIB);
        assert_eq!(normalize_memory_class(30 * GIB).expect("class"), 32 * GIB);
    }
}
