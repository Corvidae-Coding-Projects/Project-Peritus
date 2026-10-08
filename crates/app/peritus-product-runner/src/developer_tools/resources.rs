//! Resource observations and advisory defaults for developer commands.

use std::{env, path::Path, thread};

use serde_json::Value;

use super::wire::object;

const BYTES_PER_BUILD_JOB: u64 = 2 * 1024 * 1024 * 1024;
const MAX_RECOMMENDED_PARALLELISM: usize = 8;

/// One conservative resource observation used to suggest command parallelism.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CommandResources {
    logical_cpus: usize,
    effective_cpus: usize,
    cgroup_memory_limit_bytes: Option<u64>,
    cgroup_memory_headroom_bytes: Option<u64>,
    available_memory_bytes: Option<u64>,
    estimated_memory_budget_bytes: Option<u64>,
    recommended_parallelism: usize,
}

pub(super) struct CommandResourceSelection {
    environment: Vec<(String, String)>,
    observation: Value,
}

impl CommandResources {
    pub(super) fn observe() -> Self {
        let logical_cpus = thread::available_parallelism().map_or(1, usize::from);
        let effective_cpus = effective_cpu_limit().unwrap_or(logical_cpus).min(logical_cpus).max(1);
        let (cgroup_memory_limit_bytes, cgroup_memory_headroom_bytes) = cgroup_memory_limits();
        let available_memory_bytes = available_memory();
        let estimated_memory_budget_bytes = match (
            cgroup_memory_headroom_bytes,
            available_memory_bytes,
        ) {
            (Some(headroom), Some(available)) => Some(headroom.min(available)),
            (Some(headroom), None) => Some(headroom),
            (None, available) => available,
        };
        let memory_parallelism = estimated_memory_budget_bytes
            .map_or(MAX_RECOMMENDED_PARALLELISM, |bytes| {
                usize::try_from((bytes / BYTES_PER_BUILD_JOB).max(1)).unwrap_or(usize::MAX)
            });
        let recommended_parallelism =
            effective_cpus.min(memory_parallelism).clamp(1, MAX_RECOMMENDED_PARALLELISM);
        Self {
            logical_cpus,
            effective_cpus,
            cgroup_memory_limit_bytes,
            cgroup_memory_headroom_bytes,
            available_memory_bytes,
            estimated_memory_budget_bytes,
            recommended_parallelism,
        }
    }

    pub(super) fn observation(self) -> Value {
        object(vec![
            ("advisory", Value::Bool(true)),
            ("logical_cpus", Value::from(self.logical_cpus)),
            ("effective_cpus", Value::from(self.effective_cpus)),
            (
                "cgroup_memory_limit_bytes",
                self.cgroup_memory_limit_bytes.map_or(Value::Null, Value::from),
            ),
            (
                "cgroup_memory_headroom_bytes",
                self.cgroup_memory_headroom_bytes.map_or(Value::Null, Value::from),
            ),
            (
                "available_memory_bytes",
                self.available_memory_bytes.map_or(Value::Null, Value::from),
            ),
            (
                "estimated_memory_budget_bytes",
                self.estimated_memory_budget_bytes.map_or(Value::Null, Value::from),
            ),
            ("estimated_bytes_per_build_job", Value::from(BYTES_PER_BUILD_JOB)),
            (
                "recommendation_maximum",
                Value::from(MAX_RECOMMENDED_PARALLELISM),
            ),
            ("recommended_parallelism", Value::from(self.recommended_parallelism)),
        ])
    }

    pub(super) fn select(self, program: &str, arguments: &[String]) -> CommandResourceSelection {
        let explicit = requested_parallelism(program, arguments);
        let jobs = self.recommended_parallelism.to_string();
        let mut environment = vec![("PERITUS_RECOMMENDED_PARALLELISM".to_owned(), jobs.clone())];
        let mut applied = vec![Value::String("PERITUS_RECOMMENDED_PARALLELISM".to_owned())];
        let mut preserved = Vec::new();
        for (name, value) in [
            ("CARGO_BUILD_JOBS", jobs.clone()),
            ("CMAKE_BUILD_PARALLEL_LEVEL", jobs.clone()),
            ("MAKEFLAGS", format!("-j{jobs}")),
            ("GOMAXPROCS", jobs.clone()),
            ("RAYON_NUM_THREADS", jobs.clone()),
            ("NUM_JOBS", jobs.clone()),
            ("MAX_JOBS", jobs.clone()),
            ("npm_config_jobs", jobs),
        ] {
            if env::var_os(name).is_some() {
                preserved.push(Value::String(name.to_owned()));
            } else if explicit.is_none() {
                environment.push((name.to_owned(), value));
                applied.push(Value::String(name.to_owned()));
            }
        }
        let mut observation = self.observation();
        if let Some(fields) = observation.as_object_mut() {
            fields.insert("applied_environment_defaults".to_owned(), Value::Array(applied));
            fields.insert(
                "explicit_parallelism".to_owned(),
                explicit.map_or(Value::Null, RequestedParallelism::observation),
            );
            fields.insert("preserved_environment_settings".to_owned(), Value::Array(preserved));
        }
        CommandResourceSelection { environment, observation }
    }
}

impl CommandResourceSelection {
    pub(super) fn into_parts(self) -> (Vec<(String, String)>, Value) {
        (self.environment, self.observation)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RequestedParallelism<'a> {
    syntax: &'a str,
    jobs: Option<usize>,
}

impl RequestedParallelism<'_> {
    fn observation(self) -> Value {
        object(vec![
            ("jobs", self.jobs.map_or(Value::Null, Value::from)),
            ("source", Value::String("command_argument".to_owned())),
            ("syntax", Value::String(self.syntax.to_owned())),
        ])
    }
}

#[derive(Clone, Copy)]
enum BuildTool {
    Cargo,
    Cmake,
    Make,
    Ninja,
}

fn requested_parallelism<'a>(
    program: &str,
    arguments: &'a [String],
) -> Option<RequestedParallelism<'a>> {
    let name = Path::new(program).file_name()?.to_str()?.to_ascii_lowercase();
    let name = name
        .strip_suffix(".exe")
        .or_else(|| name.strip_suffix(".cmd"))
        .or_else(|| name.strip_suffix(".bat"))
        .unwrap_or(&name);
    let tool = match name {
        "cargo" => BuildTool::Cargo,
        "cmake" => BuildTool::Cmake,
        "make" | "gmake" => BuildTool::Make,
        "ninja" | "ninja-build" => BuildTool::Ninja,
        _ => return None,
    };
    for (index, argument) in arguments.iter().enumerate() {
        if argument == "-j" {
            return Some(RequestedParallelism {
                syntax: "-j",
                jobs: arguments.get(index + 1).and_then(|value| value.parse().ok()),
            });
        }
        if let Some(value) = argument.strip_prefix("-j").filter(|value| !value.is_empty()) {
            return Some(RequestedParallelism { syntax: "-jN", jobs: value.parse().ok() });
        }
        let long = match tool {
            BuildTool::Cargo | BuildTool::Make => "--jobs",
            BuildTool::Cmake => "--parallel",
            BuildTool::Ninja => continue,
        };
        if argument == long {
            return Some(RequestedParallelism {
                syntax: long,
                jobs: arguments.get(index + 1).and_then(|value| value.parse().ok()),
            });
        }
        if let Some(value) = argument.strip_prefix(long).and_then(|value| value.strip_prefix('=')) {
            return Some(RequestedParallelism { syntax: long, jobs: value.parse().ok() });
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn effective_cpu_limit() -> Option<usize> {
    cgroup_directories()
        .into_iter()
        .filter_map(|path| std::fs::read_to_string(path.join("cpu.max")).ok())
        .filter_map(|text| parse_cpu_max(&text))
        .min()
}

#[cfg(not(target_os = "linux"))]
const fn effective_cpu_limit() -> Option<usize> {
    None
}

#[cfg(target_os = "linux")]
fn cgroup_memory_limits() -> (Option<u64>, Option<u64>) {
    let mut limit = None;
    let mut headroom = None;
    for directory in cgroup_directories() {
        let Some(maximum) = std::fs::read_to_string(directory.join("memory.max"))
            .ok()
            .and_then(|text| parse_memory_max(&text))
        else {
            continue;
        };
        limit = Some(limit.map_or(maximum, |current: u64| current.min(maximum)));
        if let Some(current) = std::fs::read_to_string(directory.join("memory.current"))
            .ok()
            .and_then(|text| text.trim().parse::<u64>().ok())
        {
            let remaining = maximum.saturating_sub(current);
            headroom = Some(headroom.map_or(remaining, |prior: u64| prior.min(remaining)));
        }
    }
    (limit, headroom)
}

#[cfg(not(target_os = "linux"))]
const fn cgroup_memory_limits() -> (Option<u64>, Option<u64>) {
    (None, None)
}

#[cfg(target_os = "linux")]
fn available_memory() -> Option<u64> {
    std::fs::read_to_string("/proc/meminfo").ok().and_then(|text| parse_mem_available(&text))
}

#[cfg(not(target_os = "linux"))]
const fn available_memory() -> Option<u64> {
    None
}

#[cfg(target_os = "linux")]
fn cgroup_directories() -> Vec<std::path::PathBuf> {
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
        paths.push(current.clone());
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
            cgroup_memory_limit_bytes: Some(8 * 1024 * 1024 * 1024),
            cgroup_memory_headroom_bytes: Some(4 * 1024 * 1024 * 1024),
            available_memory_bytes: Some(6 * 1024 * 1024 * 1024),
            estimated_memory_budget_bytes: Some(4 * 1024 * 1024 * 1024),
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
            Some(RequestedParallelism { syntax: "--parallel", jobs: Some(24) }),
        );
        assert_eq!(
            requested_parallelism("gmake", &["-j8".into()]),
            Some(RequestedParallelism { syntax: "-jN", jobs: Some(8) }),
        );
        assert_eq!(
            requested_parallelism("cargo", &["--jobs=3".into()]),
            Some(RequestedParallelism { syntax: "--jobs", jobs: Some(3) }),
        );
        assert_eq!(
            requested_parallelism(
                "cmake",
                &["--build".into(), "build".into(), "--parallel".into(), "--target".into()]
            ),
            Some(RequestedParallelism { syntax: "--parallel", jobs: None }),
        );
        assert_eq!(
            requested_parallelism("make", &["-j".into(), "all".into()]),
            Some(RequestedParallelism { syntax: "-j", jobs: None }),
        );
        assert_eq!(requested_parallelism("python", &["-j24".into()]), None);
    }

    #[test]
    fn explicit_parallelism_is_preserved_as_advisory_evidence() {
        let (_, observation) = resources(1)
            .select(
                "cmake",
                &["--build".into(), "build".into(), "--parallel".into(), "24".into()],
            )
            .into_parts();
        assert_eq!(observation["recommended_parallelism"], 1);
        assert_eq!(observation["explicit_parallelism"]["jobs"], 24);
        assert_eq!(observation["advisory"], true);
    }

    #[test]
    fn admitted_commands_receive_cross_language_parallelism_defaults() {
        let environment = resources(3)
            .select("cmake", &["--build".into(), "build".into()])
            .into_parts()
            .0
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(environment["CMAKE_BUILD_PARALLEL_LEVEL"], "3");
        assert_eq!(environment["CARGO_BUILD_JOBS"], "3");
        assert_eq!(environment["MAKEFLAGS"], "-j3");
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
