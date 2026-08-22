use super::workflow_commands::parse_script;

pub(crate) fn is_exact_package_gate(script: &str, package: &str, class: &str) -> bool {
    let parsed = parse_script(script);
    if !parsed.is_failure_propagating() || parsed.commands().len() != 1 {
        return false;
    }
    let command = &parsed.commands()[0];
    let test = ["test", "--package", package, "--all-targets", "--all-features", "--locked"];
    let no_cheating = [
        "verus",
        "verify",
        "--package",
        package,
        "--all-features",
        "--locked",
        "--check-toolchain",
        "--fwd-verus-args-to",
        "roots",
        "--",
        "--no-cheating",
        "--rlimit",
        "20",
    ];
    let trusted = [
        "verus",
        "verify",
        "--package",
        package,
        "--all-features",
        "--locked",
        "--check-toolchain",
        "--fwd-verus-args-to",
        "roots",
        "--",
        "--rlimit",
        "20",
    ];
    let verify = match class {
        "V" | "H" => command.is_exact_cargo(&no_cheating),
        "T" => command.is_exact_cargo(&trusted),
        _ => false,
    };
    !command.has_leading_assignments() && (command.is_exact_cargo(&test) || verify)
}

#[cfg(test)]
mod tests {
    use super::is_exact_package_gate;

    #[test]
    fn accepts_only_full_locked_package_evidence() {
        assert!(is_exact_package_gate(
            "cargo test --package peritus-types --all-targets --all-features --locked",
            "peritus-types",
            "V",
        ));
        assert!(is_exact_package_gate(
            "cargo verus verify --package peritus-types --all-features --locked \
             --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20",
            "peritus-types",
            "V",
        ));
        assert!(is_exact_package_gate(
            "cargo verus verify --package peritus-tcb --all-features --locked \
             --check-toolchain --fwd-verus-args-to roots -- --rlimit 20",
            "peritus-tcb",
            "T",
        ));
        assert!(!is_exact_package_gate(
            "cargo verus verify --package peritus-tcb --all-features --locked \
             --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20",
            "peritus-tcb",
            "T",
        ));
        assert!(!is_exact_package_gate(
            "cargo verus verify --package peritus-types --all-features --locked \
             --check-toolchain --fwd-verus-args-to roots -- --rlimit 20",
            "peritus-types",
            "V",
        ));
        for rejected in [
            "cargo test --package peritus-types --all-targets --all-features",
            "cargo test --package another --all-targets --all-features --locked",
            "cargo verus verify --package peritus-types --all-features --locked -- --no-cheating",
            "cargo test --package peritus-types --all-targets --all-features --locked || true",
        ] {
            assert!(
                !is_exact_package_gate(rejected, "peritus-types", "V"),
                "accepted `{rejected}`"
            );
        }
    }
}
