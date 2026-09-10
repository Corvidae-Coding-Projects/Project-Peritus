//! Bounded hosted execution with explicit compilation and native-qualification allowances.

use crate::error::Diagnostic;
use std::path::Path;
use yaml_rust2::Yaml;
use yaml_rust2::yaml::Hash;

pub(super) fn validate(
    mapping: &Hash,
    path: &Path,
    location: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let maximum = if path == Path::new(".github/workflows/release.yml")
        && matches!(
            location,
            "jobs.build-daemon-library"
                | "jobs.build-cli-library"
                | "jobs.build-binary"
                | "jobs.check-native-staging"
                | "jobs.distro-compile"
                | "jobs.distro-compile-checks"
        ) {
        20
    } else if [
        (".github/workflows/ci.yml", "jobs.rust"),
        (".github/workflows/formal-governance.yml", "jobs.rust-shards"),
        (".github/workflows/security-qualification.yml", "jobs.native-security"),
        (".github/workflows/product-package.yml", "jobs.build-h2-binary"),
    ]
    .iter()
    .any(|(workflow, job)| path == Path::new(workflow) && location == *job)
    {
        15
    } else {
        10
    };
    let timeout = mapping.get(&Yaml::String("timeout-minutes".into())).and_then(Yaml::as_i64);
    if !timeout.is_some_and(|minutes| (1..=maximum).contains(&minutes)) {
        diagnostics.push(Diagnostic::at(
            path,
            format!("`{location}` does not have a timeout from 1 through {maximum} minutes"),
            "keep ordinary jobs within ten minutes, named Rust, H0, and native binary builds within fifteen, and named release compilation jobs within twenty",
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::super::reproducibility_workflow_tests::{assert_message, validate};
    use super::super::workflow_files::DocumentKind;

    const RELEASE_COMPILATION_JOBS: [&str; 6] = [
        "build-daemon-library",
        "build-cli-library",
        "build-binary",
        "check-native-staging",
        "distro-compile",
        "distro-compile-checks",
    ];

    fn timeout_workflow(job: &str, timeout: &str) -> String {
        format!(
            "name: timeout policy\njobs:\n  {job}:\n    runs-on: ubuntu-24.04\n    timeout-minutes: {timeout}\n    steps:\n      - run: cargo test --workspace --locked\n"
        )
    }

    #[test]
    fn qualification_allowance_is_bounded_and_scoped_to_exact_jobs() {
        let allowed = [
            (".github/workflows/ci.yml", "jobs.rust"),
            (".github/workflows/formal-governance.yml", "jobs.rust-shards"),
            (".github/workflows/security-qualification.yml", "jobs.native-security"),
            (".github/workflows/product-package.yml", "jobs.build-h2-binary"),
        ];
        for (path, job) in allowed {
            let path = std::path::Path::new(".github/workflows")
                .join(std::path::Path::new(path).file_name().expect("workflow filename"));
            for (minutes, valid) in [(1, true), (10, true), (15, true), (0, false), (16, false)] {
                let mapping = yaml_rust2::yaml::Hash::from_iter([(
                    yaml_rust2::Yaml::String("timeout-minutes".into()),
                    yaml_rust2::Yaml::Integer(minutes),
                )]);
                let mut diagnostics = Vec::new();
                super::validate(&mapping, &path, job, &mut diagnostics);
                assert_eq!(diagnostics.is_empty(), valid, "{path:?}/{job}/{minutes}");
            }
        }
        for (path, job) in [
            (".github/workflows/extra.yml", "jobs.native-security"),
            (".github/workflows/security-qualification.yml", "jobs.rust"),
            (".github/workflows/ci.yml", "jobs.rust-shards"),
            (".github/workflows/formal-governance.yml", "jobs.policy"),
        ] {
            let mapping = yaml_rust2::yaml::Hash::from_iter([(
                yaml_rust2::Yaml::String("timeout-minutes".into()),
                yaml_rust2::Yaml::Integer(15),
            )]);
            let mut diagnostics = Vec::new();
            super::validate(&mapping, std::path::Path::new(path), job, &mut diagnostics);
            assert_message(&diagnostics, "timeout from 1 through 10 minutes");
        }
    }

    #[test]
    fn named_release_compilation_jobs_accept_up_to_twenty_minutes() {
        for job in RELEASE_COMPILATION_JOBS {
            for timeout in ["1", "10", "11", "20"] {
                let (_, diagnostics) = validate(
                    ".github/workflows/release.yml",
                    DocumentKind::Workflow,
                    &timeout_workflow(job, timeout),
                );
                assert!(diagnostics.is_empty(), "{job}/{timeout}: {diagnostics:?}");
            }
        }
    }

    #[test]
    fn release_compilation_timeouts_reject_unbounded_or_noninteger_values() {
        for job in RELEASE_COMPILATION_JOBS {
            for timeout in ["0", "-1", "21", "\"20\"", "null", "", "1.5", "${{ inputs.timeout }}"] {
                let (_, diagnostics) = validate(
                    ".github/workflows/release.yml",
                    DocumentKind::Workflow,
                    &timeout_workflow(job, timeout),
                );
                assert_message(&diagnostics, "timeout from 1 through 20 minutes");
            }
        }
    }

    #[test]
    fn release_timeout_exception_cannot_move_to_other_workflows_or_jobs() {
        for job in RELEASE_COMPILATION_JOBS {
            for path in [".github/workflows/extra.yml", ".github/workflows/release.yaml"] {
                let (_, diagnostics) =
                    validate(path, DocumentKind::Workflow, &timeout_workflow(job, "20"));
                assert_message(&diagnostics, "timeout from 1 through 10 minutes");
            }
        }
        for job in [
            "policy",
            "h2",
            "bootstrap",
            "assemble",
            "compare-native",
            "attest",
            "distro-image",
            "distro-build",
            "distro-sign",
            "stage-draft",
            "build-h2-controller",
            "build-cli-library-extra",
        ] {
            let (_, diagnostics) = validate(
                ".github/workflows/release.yml",
                DocumentKind::Workflow,
                &timeout_workflow(job, "20"),
            );
            assert_message(&diagnostics, "timeout from 1 through 10 minutes");
        }
    }
}
