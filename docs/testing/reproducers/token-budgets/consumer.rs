//! Actual public WorkingState consumer; no unit framework, model, or replacement implementation.
use peritus_codec::sha256;
use peritus_context::working::*;
use peritus_context::{
    AuthorityClass, ContextErrorKind, ContextLimits, ContextNodeId, MessageRole, TrustClass,
    bind_context_content,
};
use peritus_role::HarnessRole;
use peritus_types::{RunId, WorkspaceId};
use serde_json::json;
use std::{collections::BTreeSet, error::Error, fs, path::Path};

fn id(n: usize) -> ContextNodeId {
    ContextNodeId::new((n as u128).to_be_bytes()).unwrap()
}
fn binding(role: HarnessRole) -> WorkingBinding {
    WorkingBinding::new(
        RunId::new([1; 16]).unwrap(),
        WorkspaceId::new([2; 16]).unwrap(),
        id(3),
        role,
        17,
    )
}
fn fixture(
    root: &Path,
    count: usize,
    last: usize,
    unicode: bool,
    closure: bool,
    role: HarnessRole,
) -> WorkingState {
    fs::create_dir_all(root).unwrap();
    let limits = WorkingLimits::standard();
    let binding = binding(role);
    let texts: Vec<_> = (1..=count)
        .map(|n| {
            let mut text = format!("REQUIRED-MARKER-{n:03} ");
            if unicode {
                text.push_str("🦀 café e\u{301} 日本語 العربية ");
            }
            text.push_str(&"x".repeat(if n == count { last } else { 1450 }));
            text
        })
        .collect();
    let artifact = texts.join("\n").into_bytes();
    fs::write(root.join("observed-source.txt"), &artifact).unwrap();
    let read = fs::read(root.join("observed-source.txt")).unwrap();
    assert_eq!(read, artifact);
    let digest = sha256(&read);
    let mut state = WorkingState::new(
        WorkingEnvironment::new(binding, sha256(b"candidate"), vec![], limits).unwrap(),
        limits,
    )
    .unwrap();
    state = ingest_working_observation(
        &state,
        binding,
        ObservationSource::new(
            ObservationId::new(1).unwrap(),
            digest,
            read.len() as u64,
            0,
            read.len() as u64,
            ObservationKind::ToolOutput,
        )
        .unwrap(),
    )
    .unwrap();
    let content_limits = ContextLimits::new(512, 2048, 32, 5).unwrap();
    for (n, text) in texts.iter().enumerate() {
        let number = n + 1;
        let dependency = closure && number % 2 == 0;
        let leaf = closure && number % 2 == 1;
        let links = WorkingLinks::new(
            vec![ObservationId::new(1).unwrap()],
            vec![],
            if dependency { vec![id(number - 1)] } else { vec![] },
            limits,
        )
        .unwrap();
        let kind = if leaf {
            WorkingEntryKind::Observation
        } else if number % 4 == 0 {
            WorkingEntryKind::FailedApproach
        } else {
            WorkingEntryKind::Plan
        };
        let content = text.as_bytes().to_vec();
        let mut entry = WorkingEntry::new(
            id(number),
            kind,
            bind_context_content(content.clone(), sha256(&content), content_limits).unwrap(),
            links,
            WorkingValidity::new(None, None, vec![], limits).unwrap(),
            limits,
        )
        .unwrap();
        if leaf {
            entry = entry.with_status(WorkingEntryStatus::Resolved).unwrap();
        }
        state = apply_working_delta(
            &state,
            &WorkingDelta::new(binding, state.revision(), vec![entry], limits).unwrap(),
        )
        .unwrap();
    }
    assert_eq!(fs::read(root.join("observed-source.txt")).unwrap(), read);
    assert_eq!(
        sha256(&read),
        state.observation(binding, ObservationId::new(1).unwrap()).unwrap().artifact()
    );
    state
}
fn exact_boundary(root: &Path) -> WorkingState {
    // Concrete minimized original input; see exact-boundary-fixture.json.
    fixture(root, 29, 1725, false, false, HarnessRole::Writer)
}
fn independent_oracle(state: &WorkingState, view: &WorkingRenderView, unicode: bool) {
    let entries = state.entries(state.binding()).unwrap();
    let plan = view.plan().expect("nonempty working state must render");
    assert!(view.omitted().is_empty(), "required entries or their dependency closure omitted");
    assert_eq!(plan.segments().len(), entries.len());
    let observed: BTreeSet<_> = plan.segments().iter().map(|s| s.source_id()).collect();
    let expected: BTreeSet<_> = entries.iter().map(|e| e.id()).collect();
    assert_eq!(observed, expected, "exact required and dependency IDs");
    for entry in entries {
        let segment = plan.segments().iter().find(|s| s.source_id() == entry.id()).unwrap();
        let text = std::str::from_utf8(segment.content()).unwrap();
        let original = std::str::from_utf8(entry.content().bytes()).unwrap();
        assert!(
            text.contains(&format!("{original:?}")),
            "entry content identity lost"
        );
        assert!(text.contains("obs:000001"), "source locator lost");
        if unicode {
            assert!(text.contains("🦀") && text.contains("日本語"), "Unicode evidence damaged");
        }
        assert_eq!(segment.authority(), AuthorityClass::NonAuthoritative);
        assert_eq!(segment.trust(), TrustClass::Untrusted);
        assert_eq!(segment.message_role(), MessageRole::Evidence);
        for dependency in entry.links().depends_on() {
            assert!(observed.contains(dependency), "dependency disappeared");
        }
    }
}
fn main() -> Result<(), Box<dyn Error>> {
    let root = std::env::args().nth(1).expect("usage: consumer FRESH_OUTPUT_DIRECTORY");
    let root = Path::new(&root);
    if root.exists() {
        return Err("Use a fresh output directory".into());
    }
    fs::create_dir_all(root)?;
    let exact = exact_boundary(&root.join("exact-boundary"));
    let before = encode_working_state(&exact)?;
    fs::write(root.join("exact-boundary/state.pwms"), &before)?;
    let error = render_working_state_with_headroom(&exact, exact.binding(), 16709, 16709)
        .err()
        .expect("legacy boundary must fail");
    if let WorkingError::Projection(inner) = error {
        assert_eq!(inner.kind(), ContextErrorKind::RequiredTokenBudgetExceeded);
        assert_eq!(inner.expected(), Some(16709));
        assert_eq!(inner.actual(), Some(16903));
    } else {
        panic!("unexpected legacy rejection {error:?}");
    }
    assert_eq!(encode_working_state(&exact)?, before, "rejected render changed durable state");
    println!(
        "{}",
        json!({"case":"original-explicit-budget-boundary","expected":16709,"actual":16903,"result":"reproduced","entries":exact.entries(exact.binding())?.len()})
    );
    #[cfg(feature = "complete-api")]
    {
        for (name, count, unicode, closure) in [
            ("required-ascii", 96, false, false),
            ("required-unicode-closure", 96, true, true),
            ("all-512-entries", 512, false, true),
        ] {
            let state =
                fixture(&root.join(name), count, 1450, unicode, closure, HarnessRole::Writer);
            let bytes = encode_working_state(&state)?;
            fs::write(root.join(name).join("state.pwms"), &bytes)?;
            let restored = decode_working_state(
                &fs::read(root.join(name).join("state.pwms"))?,
                state.binding(),
                WorkingLimits::standard(),
            )?;
            assert_eq!(restored, state);
            let view = render_working_state_complete(&state, state.binding())?;
            independent_oracle(&state, &view, unicode);
            assert!(view.plan().unwrap().accounting().used_input() > 16709);
            let again = render_working_state_complete(&restored, restored.binding())?;
            assert_eq!(format!("{:?}", view.plan()), format!("{:?}", again.plan()));
            assert_eq!(
                encode_working_state(&state)?,
                bytes,
                "complete render mutated durable state"
            );
            assert_eq!(encode_working_state(&restored)?, bytes, "restart changed durable state");
            assert!(matches!(
                render_working_state_complete(&state, binding(HarnessRole::Reviewer)),
                Err(WorkingError::BindingMismatch)
            ));
            println!(
                "{}",
                json!({"case":name,"result":"PASS","selected":count,"omitted":view.omitted().len(),"input_tokens_accounting":view.plan().unwrap().accounting().used_input(),"state_sha256":format!("{:?}",sha256(&bytes)),"state_unchanged":true,"restart_identical":true,"cross_role_rejected":true})
            );
        }
        independent_oracle(&exact, &render_working_state_complete(&exact, exact.binding())?, false);
        let mut seed = 0x50e71_75_u64;
        for case in 0..24 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let count = 24 + 2 * ((seed >> 32) as usize % 9);
            let unicode = seed & 1 != 0;
            let closure = seed & 2 != 0;
            let role = if case % 6 == 0 { HarnessRole::Reviewer } else { HarnessRole::Writer };
            let state = fixture(
                &root.join(format!("generated-{case:02}")),
                count,
                600 + (seed as usize % 1100),
                unicode,
                closure,
                role,
            );
            let before = encode_working_state(&state)?;
            let restored =
                decode_working_state(&before, state.binding(), WorkingLimits::standard())?;
            assert_eq!(restored, state);
            let view = render_working_state_complete(&restored, restored.binding())?;
            if role == HarnessRole::Reviewer {
                assert!(view.plan().is_none());
                assert_eq!(view.omitted().len(), count);
            } else {
                independent_oracle(&restored, &view, unicode);
            }
            assert_eq!(encode_working_state(&state)?, before);
            assert_eq!(encode_working_state(&restored)?, before);
            println!(
                "{}",
                json!({"case":"generated","index":case,"seed":seed,"entries":count,"unicode":unicode,"closure":closure,"role":format!("{role:?}"),"selected":view.plan().map_or(0,|p|p.segments().len()),"accounting_tokens":view.plan().map_or(0,|p|p.accounting().used_input()),"result":"PASS"})
            );
        }
        let limits = WorkingLimits::standard();
        let empty = WorkingState::new(
            WorkingEnvironment::new(
                binding(HarnessRole::Writer),
                sha256(b"empty"),
                vec![],
                limits,
            )?,
            limits,
        )?;
        let empty_before = encode_working_state(&empty)?;
        let view = render_working_state_complete(&empty, empty.binding())?;
        assert!(view.plan().is_none());
        assert!(view.omitted().is_empty());
        assert_eq!(encode_working_state(&empty)?, empty_before);
        println!("{}", json!({"case":"empty-working-state","result":"PASS"}));
        let reviewer = fixture(&root.join("reviewer"), 96, 1450, true, true, HarnessRole::Reviewer);
        let before = encode_working_state(&reviewer)?;
        let view = render_working_state_complete(&reviewer, reviewer.binding())?;
        assert!(view.plan().is_none());
        assert_eq!(view.omitted().len(), 96);
        assert_eq!(encode_working_state(&reviewer)?, before);
        println!(
            "{}",
            json!({"case":"reviewer-isolation","result":"PASS","selected":0,"excluded":96,"state_unchanged":true})
        );
    }
    Ok(())
}
