#[path = "../../../../../crates/app/peritus-launcher/src/update/release/version.rs"]
mod release;

pub(super) fn version(tag: &str) -> (Option<String>, Option<String>) {
    match release::Version::from_tag(tag) {
        Ok(version) => (Some(version.render()), None),
        Err(crate::LauncherError::Update(message)) => (None, Some(message)),
    }
}
