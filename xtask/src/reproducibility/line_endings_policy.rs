use super::policy_file;
use crate::error::Diagnostic;
use std::path::Path;

const PATH: &str = ".gitattributes";
const REVIEWED: &str = "* text=auto eol=lf\n";

pub(super) fn validate(root: &Path, diagnostics: &mut Vec<Diagnostic>) {
    let Some(contents) = policy_file::read_regular(
        root,
        Path::new(PATH),
        "line-ending policy is missing, non-regular, or symbolic",
        "line-ending policy",
        "restore the reviewed regular .gitattributes file",
        diagnostics,
    ) else {
        return;
    };
    if contents != REVIEWED {
        diagnostics.push(Diagnostic::at(
            PATH,
            "line-ending policy differs from the reviewed cross-platform contract",
            "restore exactly `* text=auto eol=lf` so every runner checks out Rust and policy inputs as LF",
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::{REVIEWED, validate};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn exact_lf_policy_is_required() {
        let fixture = Fixture::new();
        fixture.write(REVIEWED);
        let mut diagnostics = Vec::new();
        validate(&fixture.root, &mut diagnostics);
        assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");

        fixture.write("* text=auto\n");
        validate(&fixture.root, &mut diagnostics);
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.message().contains("differs from the reviewed cross-platform contract")
        }));
    }

    #[test]
    fn missing_policy_fails_closed() {
        let fixture = Fixture::new();
        let mut diagnostics = Vec::new();
        validate(&fixture.root, &mut diagnostics);
        assert!(
            diagnostics.iter().any(|diagnostic| diagnostic
                .message()
                .contains("missing, non-regular, or symbolic"))
        );
    }

    struct Fixture {
        root: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "peritus-line-endings-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root).expect("fixture root must be creatable");
            Self { root }
        }

        fn write(&self, contents: &str) {
            fs::write(self.root.join(".gitattributes"), contents)
                .expect("line-ending policy fixture must be writable");
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).expect("fixture root must be removable");
        }
    }
}
