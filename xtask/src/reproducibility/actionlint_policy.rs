use super::policy_file;
use crate::error::Diagnostic;
use std::path::Path;

const PATH: &str = ".github/actionlint.yaml";
const REVIEWED: &str = "paths: {}\n";

pub(super) fn validate(root: &Path, diagnostics: &mut Vec<Diagnostic>) {
    let Some(contents) = policy_file::read_regular(
        root,
        Path::new(PATH),
        "actionlint configuration is missing, non-regular, or symbolic",
        "actionlint configuration",
        "restore the reviewed regular configuration with no diagnostic suppressions",
        diagnostics,
    ) else {
        return;
    };
    if contents != REVIEWED {
        diagnostics.push(Diagnostic::at(
            PATH,
            "actionlint configuration differs from the exact reviewed no-suppression policy",
            "restore the exact empty paths mapping; every actionlint diagnostic must remain visible",
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::{REVIEWED, validate};
    use std::fs;

    #[test]
    fn policy_suppresses_no_actionlint_diagnostics() {
        assert_eq!(REVIEWED, "paths: {}\n");
    }

    #[test]
    fn missing_configuration_retains_the_reproducibility_diagnostic() {
        let root =
            std::env::temp_dir().join(format!("peritus-missing-actionlint-{}", std::process::id()));
        fs::create_dir(&root).expect("fixture root must be creatable");
        let mut diagnostics = Vec::new();

        validate(&root, &mut diagnostics);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message().contains("missing, non-regular, or symbolic"));
        fs::remove_dir(root).expect("fixture root must be removable");
    }
}
