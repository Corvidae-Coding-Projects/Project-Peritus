//! Resource-aware defaults and admission for developer commands.

use std::{path::Path, process::Command, thread};

use peritus_agent::DeveloperLoopError;
use serde_json::Value;

use super::{path::tool, wire::object};

const BYTES_PER_BUILD_JOB: u64 = 2 * 1024 * 1024 * 1024;
const MAX_RECOMMENDED_PARALLELISM: usize = 8;

/// One conservative execution envelope observed before a model can run commands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CommandResources {
    logical_cpus: usize,
    effective_cpus: usize,
    memory_ceiling_bytes: Option<u64>,
    recommended_parallelism: usize,
}

impl CommandResources {
    pub(super) fn observe() -> Self {
        let logical_cpus = thread::available_parallelism().map_or(1, usize::from);
        let effective_cpus = effective_cpu_limit().unwrap_or(logical_cpus).min(logical_cpus).max(1);
        let memory_ceiling_bytes = effective_memory_limit();
        let memory_parallelism = memory_ceiling_bytes
            .map_or(MAX_RECOMMENDED_PARALLELISM, |bytes| {
                usize::try_from((bytes / BYTES_PER_BUILD_JOB).max(1)).unwrap_or(usize::MAX)
            });
        let recommended_parallelism =
            effective_cpus.min(memory_parallelism).clamp(1, MAX_RECOMMENDED_PARALLELISM);
        Self { logical_cpus, effective_cpus, memory_ceiling_bytes, recommended_parallelism }
    }

    pub(super) fn observation(self) -> Value {
        object(vec![
            ("logical_cpus", Value::from(self.logical_cpus)),
            ("effective_cpus", Value::from(self.effective_cpus)),
            ("memory_ceiling_bytes", self.memory_ceiling_bytes.map_or(Value::Null, Value::from)),
            ("recommended_parallelism", Value::from(self.recommended_parallelism)),
        ])
    }

    pub(super) fn prepare(
        self,
        command: &mut Command,
        program: &str,
        arguments: &[String],
    ) -> Result<(), DeveloperLoopError> {
        if let Some(requested) = requested_parallelism(program, arguments)
            && requested > self.recommended_parallelism
        {
            return Err(tool(format!(
                "command requests {requested} parallel jobs above the observed execution ceiling {}; retry with at most {} jobs or omit the explicit job count so the harness defaults apply (effective CPUs: {}, memory ceiling bytes: {})",
                self.recommended_parallelism,
                self.recommended_parallelism,
                self.effective_cpus,
                self.memory_ceiling_bytes
                    .map_or_else(|| "unknown".to_owned(), |value| value.to_string()),
            )));
        }

        let jobs = self.recommended_parallelism.to_string();
        let cargo_jobs = self.recommended_parallelism.min(2).to_string();
        command
            .env("PERITUS_RECOMMENDED_PARALLELISM", &jobs)
            .env("CARGO_BUILD_JOBS", cargo_jobs)
            .env("CMAKE_BUILD_PARALLEL_LEVEL", &jobs)
            .env("MAKEFLAGS", format!("-j{jobs}"))
            .env("GOMAXPROCS", &jobs)
            .env("RAYON_NUM_THREADS", &jobs)
            .env("NUM_JOBS", &jobs)
            .env("MAX_JOBS", &jobs)
            .env("npm_config_jobs", &jobs);
        Ok(())
    }
}

fn requested_parallelism(program: &str, arguments: &[String]) -> Option<usize> {
    let name = Path::new(program).file_name()?.to_str()?.to_ascii_lowercase();
    if !matches!(name.as_str(), "cargo" | "cmake" | "make" | "gmake" | "ninja" | "ninja-build") {
        return None;
    }
    let mut index = 0;
    while index < arguments.len() {
        let argument = &arguments[index];
        if matches!(argument.as_str(), "-j" | "--jobs" | "--parallel") {
            let requested = arguments.get(index + 1).and_then(|value| value.parse().ok());
            if argument == "--parallel" && name == "cmake" {
                return requested;
            }
            return requested.or(Some(usize::MAX));
        }
        for prefix in ["-j", "--jobs=", "--parallel="] {
            if let Some(value) = argument.strip_prefix(prefix)
                && !value.is_empty()
                && let Ok(parsed) = value.parse()
            {
                return Some(parsed);
            }
        }
        index += 1;
    }
    None
}

#[cfg(target_os = "linux")]
fn effective_cpu_limit() -> Option<usize> {
    cgroup_files("cpu.max")
        .into_iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .filter_map(|text| parse_cpu_max(&text))
        .min()
}

#[cfg(not(target_os = "linux"))]
const fn effective_cpu_limit() -> Option<usize> {
    None
}

#[cfg(target_os = "linux")]
fn effective_memory_limit() -> Option<u64> {
    let cgroup = cgroup_files("memory.max")
        .into_iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .filter_map(|text| parse_memory_max(&text))
        .min();
    let available =
        std::fs::read_to_string("/proc/meminfo").ok().and_then(|text| parse_mem_available(&text));
    match (cgroup, available) {
        (Some(limit), Some(free)) => Some(limit.min(free)),
        (Some(limit), None) => Some(limit),
        (None, available) => available,
    }
}

#[cfg(not(target_os = "linux"))]
const fn effective_memory_limit() -> Option<u64> {
    None
}

#[cfg(target_os = "linux")]
fn cgroup_files(name: &'static str) -> Vec<std::path::PathBuf> {
    let root = std::path::PathBuf::from("/sys/fs/cgroup");
    let relative = std::fs::read_to_string("/proc/self/cgroup")
        .ok()
        .and_then(|text| {
            text.lines()
                .find_map(|line| line.strip_prefix("0::"))
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "/".to_owned());
    let safe_relative = relative
        .trim_start_matches('/')
        .split('/')
        .filter(|component| !component.is_empty() && *component != "." && *component != "..")
        .collect::<std::path::PathBuf>();
    let mut current = root.join(safe_relative);
    let mut paths = Vec::new();
    loop {
        paths.push(current.join(name));
        if current == root || !current.pop() {
            break;
        }
    }
    paths
}

#[cfg(target_os = "linux")]
fn parse_cpu_max(text: &str) -> Option<usize> {
    let mut fields = text.split_whitespace();
    let quota = fields.next()?;
    let period = fields.next()?.parse::<u64>().ok()?;
    if quota == "max" || period == 0 {
        return None;
    }
    let quota = quota.parse::<u64>().ok()?;
    usize::try_from(quota.div_ceil(period).max(1)).ok()
}

#[cfg(target_os = "linux")]
fn parse_memory_max(text: &str) -> Option<u64> {
    let value = text.trim();
    (value != "max").then(|| value.parse().ok()).flatten()
}

#[cfg(target_os = "linux")]
fn parse_mem_available(text: &str) -> Option<u64> {
    let kib = text
        .lines()
        .find_map(|line| line.strip_prefix("MemAvailable:"))?
        .split_whitespace()
        .next()?
        .parse::<u64>()
        .ok()?;
    kib.checked_mul(1024)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resources(parallelism: usize) -> CommandResources {
        CommandResources {
            logical_cpus: 24,
            effective_cpus: parallelism,
            memory_ceiling_bytes: Some(2 * 1024 * 1024 * 1024),
            recommended_parallelism: parallelism,
        }
    }

    #[test]
    fn common_build_job_flags_are_recognized_without_inspecting_shell_text() {
        assert_eq!(
            requested_parallelism(
                "cmake",
                &["--build".into(), "build".into(), "--parallel".into(), "24".into()]
            ),
            Some(24),
        );
        assert_eq!(requested_parallelism("gmake", &["-j8".into()]), Some(8));
        assert_eq!(requested_parallelism("cargo", &["--jobs=3".into()]), Some(3));
        assert_eq!(
            requested_parallelism(
                "cmake",
                &["--build".into(), "build".into(), "--parallel".into(), "--target".into()]
            ),
            None,
        );
        assert_eq!(requested_parallelism("make", &["-j".into(), "all".into()]), Some(usize::MAX));
        assert_eq!(requested_parallelism("python", &["-j24".into()]), None);
    }

    #[test]
    fn excessive_explicit_parallelism_is_rejected_before_spawn() {
        let mut command = Command::new("cmake");
        let error = resources(1)
            .prepare(
                &mut command,
                "cmake",
                &["--build".into(), "build".into(), "--parallel".into(), "24".into()],
            )
            .expect_err("oversized build");
        assert!(error.to_string().contains("above the observed execution ceiling 1"));
        assert!(error.to_string().contains("retry with at most 1 jobs"));
    }

    #[test]
    fn admitted_commands_receive_cross_language_parallelism_defaults() {
        let mut command = Command::new("cmake");
        resources(3).prepare(&mut command, "cmake", &["--build".into(), "build".into()]).unwrap();
        let environment = command
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().into_owned(),
                    value.map(|value| value.to_string_lossy().into_owned()),
                )
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(environment["CMAKE_BUILD_PARALLEL_LEVEL"].as_deref(), Some("3"));
        assert_eq!(environment["CARGO_BUILD_JOBS"].as_deref(), Some("2"));
        assert_eq!(environment["MAKEFLAGS"].as_deref(), Some("-j3"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_resource_formats_are_parsed_conservatively() {
        assert_eq!(parse_cpu_max("100000 100000\n"), Some(1));
        assert_eq!(parse_cpu_max("150000 100000\n"), Some(2));
        assert_eq!(parse_cpu_max("max 100000\n"), None);
        assert_eq!(parse_memory_max("2147483648\n"), Some(2_147_483_648));
        assert_eq!(parse_memory_max("max\n"), None);
        assert_eq!(parse_mem_available("MemTotal: 4 kB\nMemAvailable: 3 kB\n"), Some(3072));
    }
}
