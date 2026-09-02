//! Native host-version detection and H2 contract normalization.

use std::fs;
use std::path::Path;
use std::process::Command;

use crate::XtaskError;

pub(super) fn detect(host_os: &str) -> Result<String, XtaskError> {
    match host_os {
        "linux" => {
            let path = Path::new("/proc/sys/kernel/osrelease");
            let raw = fs::read_to_string(path)
                .map_err(|error| XtaskError::io("read Linux kernel version from", path, error))?;
            normalize(&raw).ok_or_else(|| XtaskError::metadata("Linux kernel version is malformed"))
        }
        "macos" => command_version("/usr/bin/sw_vers", &["-productVersion"], false),
        "windows" => command_version(
            "powershell",
            &["-NoProfile", "-Command", "[Environment]::OSVersion.Version.ToString()"],
            true,
        ),
        _ => Err(XtaskError::metadata("native H2 qualification is unsupported here")),
    }
}

fn command_version(
    executable: &str,
    arguments: &[&str],
    windows_marketing_version: bool,
) -> Result<String, XtaskError> {
    let output = Command::new(executable).args(arguments).output().map_err(|error| {
        XtaskError::io("run host-version probe with", Path::new(executable), error)
    })?;
    if !output.status.success() {
        return Err(XtaskError::metadata("host-version probe failed"));
    }
    let raw = String::from_utf8(output.stdout)
        .map_err(|_| XtaskError::metadata("host-version probe returned non-UTF-8 output"))?;
    let version = if windows_marketing_version { normalize_windows(&raw) } else { normalize(&raw) };
    version.ok_or_else(|| XtaskError::metadata("host-version probe returned malformed output"))
}

fn normalize_windows(raw: &str) -> Option<String> {
    let fields = numeric_fields(raw)?;
    let build = *fields.get(2)?;
    if fields.first() == Some(&10) && build >= 22_000 {
        Some(format!("11.0.0.{build}"))
    } else {
        normalized_fields(fields)
    }
}

fn normalize(raw: &str) -> Option<String> {
    normalized_fields(numeric_fields(raw)?)
}

fn numeric_fields(raw: &str) -> Option<Vec<u32>> {
    let mut fields = Vec::new();
    for field in raw.trim().split('.').take(4) {
        let digits = field.chars().take_while(char::is_ascii_digit).collect::<String>();
        if digits.is_empty() {
            break;
        }
        fields.push(digits.parse().ok()?);
    }
    (!fields.is_empty()).then_some(fields)
}

fn normalized_fields(mut fields: Vec<u32>) -> Option<String> {
    if fields.len() > 4 {
        return None;
    }
    while fields.len() < 3 {
        fields.push(0);
    }
    Some(fields.iter().map(u32::to_string).collect::<Vec<_>>().join("."))
}

#[cfg(test)]
mod tests {
    use super::{normalize, normalize_windows};

    #[test]
    fn native_versions_are_reduced_to_the_h2_contract() {
        assert_eq!(normalize("7.1.8-200.fc44.x86_64\n").as_deref(), Some("7.1.8"));
        assert_eq!(normalize("15.7.1\n").as_deref(), Some("15.7.1"));
        assert_eq!(normalize("15\n").as_deref(), Some("15.0.0"));
        assert_eq!(normalize("unknown"), None);
    }

    #[test]
    fn windows_kernel_build_is_projected_to_the_supported_product_version() {
        assert_eq!(normalize_windows("10.0.26100.0\r\n").as_deref(), Some("11.0.0.26100"));
        assert_eq!(normalize_windows("11.0.0.30000\n").as_deref(), Some("11.0.0.30000"));
    }
}
