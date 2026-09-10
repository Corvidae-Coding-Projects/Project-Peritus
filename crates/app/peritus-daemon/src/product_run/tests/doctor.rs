use super::*;
use peritus_app_protocol::{DoctorQuery, DoctorStatus};

#[test]
fn diagnostics_do_not_consume_provider_responses_or_mutate_files_or_runs() {
    let repository = repository();
    let state = tempfile::tempdir().expect("state");
    let writer = scripted(0x71, "writer", complete_writer(CORRECT));
    let reviewer = scripted(0x72, "reviewer", clean_review());
    let fixer = scripted(0x73, "fixer", Vec::new());
    let workspace = WorkspaceId::new([0x75; 16]).expect("workspace");
    let service = service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
    let before = writer.responses.lock().expect("responses").len();
    let before_files = files(state.path());
    let query = DoctorQuery::new(workspace, Some(writer.profile.profile_id()));
    let report = service.doctor(query).expect("diagnostics");
    assert_eq!(report.query(), query);
    assert!(
        report.findings().iter().any(|finding| finding.check() == "provider-route"
            && finding.status() == DoctorStatus::Healthy)
    );
    assert!(report.findings().iter().any(|finding| finding.check() == "provider-authentication"
        && finding.status() == DoctorStatus::Unsupported));
    assert_eq!(writer.responses.lock().expect("responses").len(), before);
    assert!(service.inner.records.read().expect("records").is_empty());
    assert_eq!(files(state.path()), before_files);
    let missing = DoctorQuery::new(WorkspaceId::new([0x76; 16]).expect("missing"), None);
    let report = service.doctor(missing).expect("missing observations");
    assert_eq!(
        report
            .findings()
            .iter()
            .filter(|finding| finding.status() == DoctorStatus::Blocked)
            .count(),
        2
    );
}

fn files(root: &std::path::Path) -> BTreeMap<std::path::PathBuf, Vec<u8>> {
    let mut result = BTreeMap::new();
    for entry in fs::read_dir(root).expect("directory") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            result.extend(files(&path));
        } else {
            result.insert(path.clone(), fs::read(&path).expect("file"));
        }
    }
    result
}
