//! Real split output and revision invariants over complete retained preview state.

use super::*;
use crate::product_run::PreviewAggregate;
use std::io::Write as _;

const RELEASE_FILE: &str = ".peritus-preview-tail-release";
const TRAILING_OUTPUT: &str = "TAIL_RELEASED";
const DEADLINE: Duration = Duration::from_secs(10);

pub(super) fn split_prompt() {
    std::io::stdout().write_all(b"NAME?").expect("partial prompt bytes");
    std::io::stdout().flush().expect("partial prompt");
    let release = std::env::current_dir().expect("fixture working directory").join(RELEASE_FILE);
    let began = std::time::Instant::now();
    while !release.exists() {
        assert!(began.elapsed() < DEADLINE, "fixture release was not created at {release:?}");
        std::thread::sleep(Duration::from_millis(5));
    }
    println!(" {TRAILING_OUTPUT}");
    std::io::stdout().flush().expect("trailing prompt output");
}

pub(super) async fn release_and_wait(
    service: &ProductRunService,
    query: WorkbenchResultQuery,
    prompt: &WorkbenchPreviewSnapshot,
) -> WorkbenchPreviewSnapshot {
    let mut before = retained(service, query);
    let initial_revision = page(&before).result_revision();
    assert_eq!(initial_revision, prompt.result().result_revision());
    assert!(!before.outputs.values().any(|value| value.contains(TRAILING_OUTPUT)));
    // Each test launch owns a fresh repository. Use the registered workspace that supplies the
    // actual preview cwd; the child resolves the same filename through its own current_dir().
    let cwd = service.inner.workspaces.get(&query.query().workspace()).expect("preview workspace");
    let release = cwd.join(RELEASE_FILE);
    let file = fs::File::create_new(&release).expect("exclusive fixture release");
    drop(file);
    let began = std::time::Instant::now();
    loop {
        let observed = observe(service, query);
        let after = retained(service, query);
        let unchanged = assert_transition(&before, &after);
        assert!(
            page(&after).launches().iter().all(|row| row.state() == WorkbenchLaunchState::Running),
            "preview must remain live before input: {}",
            summary(&after)
        );
        let has_tail = after.outputs.values().any(|value| value.contains(TRAILING_OUTPUT));
        if has_tail && unchanged {
            assert!(
                page(&after).result_revision() > initial_revision,
                "the deliberately split trailing output must advance the prompt revision: {}",
                summary(&after)
            );
            fs::remove_file(release).expect("remove fixture release before digest checks");
            eprintln!(
                "split prompt: NAME? revision {initial_revision}, released-tail revision {}, unchanged running poll revision {}",
                page(&before).result_revision(),
                page(&after).result_revision()
            );
            return observed;
        }
        assert!(
            began.elapsed() < DEADLINE,
            "no unchanged running observation after trailing output: {}",
            summary(&after)
        );
        before = after;
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

pub(super) async fn assert_settled(
    service: &ProductRunService,
    query: WorkbenchResultQuery,
    expected: &WorkbenchPreviewSnapshot,
) {
    assert_eq!(expected.result().launches()[0].state(), WorkbenchLaunchState::Exited);
    let before = retained(service, query);
    for _ in 0..2 {
        let observed = observe(service, query);
        let after = retained(service, query);
        assert!(
            assert_transition(&before, &after) && observed == *expected,
            "settled public preview snapshot must remain exact: before {}; after {}",
            summary(&before),
            summary(&after)
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn retained(service: &ProductRunService, query: WorkbenchResultQuery) -> PreviewAggregate {
    service
        .inner
        .records
        .read()
        .expect("records")
        .get(&query.run())
        .expect("preview run")
        .preview
        .clone()
}

fn page(preview: &PreviewAggregate) -> &peritus_app_protocol::WorkbenchResultPage {
    preview.page.as_ref().expect("retained preview page")
}

fn assert_transition(before: &PreviewAggregate, after: &PreviewAggregate) -> bool {
    let previous = page(before);
    let current = page(after);
    let unchanged = before.outputs == after.outputs
        && before.errors == after.errors
        && before.truncated == after.truncated
        && previous.query() == current.query()
        && previous.control_revision() == current.control_revision()
        && previous.capability() == current.capability()
        && previous.launches() == current.launches();
    let valid = if unchanged {
        current.result_revision() == previous.result_revision()
    } else {
        current.result_revision() > previous.result_revision()
    };
    assert!(
        valid,
        "revision must track actual retained changes (unchanged={unchanged}): before {}; after {}",
        summary(before),
        summary(after)
    );
    unchanged
}

fn summary(preview: &PreviewAggregate) -> String {
    let page = page(preview);
    let streams = |values: &BTreeMap<ControlOperationId, String>| {
        values
            .iter()
            .map(|(id, value)| {
                let bytes = value.as_bytes();
                let suffix = &bytes[bytes.len().saturating_sub(96)..];
                format!(
                    "{id:?}: {} bytes, {:?}, suffix {:?}",
                    bytes.len(),
                    peritus_codec::sha256(bytes),
                    String::from_utf8_lossy(suffix)
                )
            })
            .collect::<Vec<_>>()
    };
    let launches = page
        .launches()
        .iter()
        .map(|row| (row.launch(), row.process(), row.state(), row.ready()))
        .collect::<Vec<_>>();
    format!(
        "revision {}, launches {launches:?}, stdout {:?}, stderr {:?}, truncated {:?}",
        page.result_revision(),
        streams(&preview.outputs),
        streams(&preview.errors),
        preview.truncated
    )
}
