//! Daemon-side product coding-run execution.
//! This crate coordinates concrete provider calls, managed-worktree edits, repository gates, and
//! an independent review/fix cycle. It owns no UI and grants no authority; the daemon supplies
//! already-resolved provider and workspace capabilities.
mod accounting;
pub mod attachment;
pub use peritus_tools_computer::{
    ensure_setup as computer_ensure_setup, setup as computer_setup, status as computer_status,
};
mod attachment_read;

mod budget;

pub mod bundled_skills;

pub(crate) mod bundle;

mod candidate;
mod context_config;
pub mod context_projection;
pub mod control;
mod conversation_mode;

mod delivery_requirement;

mod design;
pub(crate) mod developer_tools;

mod diagnostic;
mod discard_recovery;

mod efficiency;
mod engineering_workflow;
mod error;

mod execution;

pub(crate) mod failover;

mod file_metadata;
mod file_observation;

pub(crate) mod gates;

mod guided_design;

mod local_context;

mod model_output;

mod progress;

pub mod qualification;

mod review;

mod reviewer_turn;

pub(crate) mod trace;

mod turn;

pub mod tool_output;

mod workspace_delivery;
pub use workspace_delivery::scope::ScopedBaseline;

mod workspace_filter;
mod workspace_kind;

mod workspace_media;

pub use workspace_media::{WorkspaceImageDiscovery, WorkspaceImagePage};

mod api;

pub use api::*;

pub use trace::{DeveloperTraceFrameKind, TraceFrame, TraceReadError, TraceReader, read_frames};

pub(crate) fn encode_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}
#[cfg(test)]
#[path = "../../../../discovery/proptest-praxis/src/fingerprint.rs"]
mod praxis_fingerprint;

#[cfg(test)]
mod prompt_policy_contract {
    use std::{fs, path::Path, time::Duration};

    use peritus_model_protocol::{
        BoundedText, ContentBlock, Message, ProtocolLimits, Role, decode_messages, encode_messages,
    };

    use crate::ConversationMode;

    const DOMAIN_MARKER: &str = "prompt-vitamins-domain-design-v2";
    const TASK_SENTINEL: &str = "CURRENT-USER-TASK-SENTINEL-8317";
    const DOMAIN_SECTIONS: [&str; 6] = [
        "## Implicit Domain Knowledge and Engineering Obligations",
        "### Mandatory behavior",
        "### Research before architecture",
        "### Supplied design expansion",
        "### Question discipline",
        "### Governing principle",
    ];

    fn count(text: &str, needle: &str) -> usize {
        text.match_indices(needle).count()
    }

    fn archive(role: &str, system: String, expects_domain_policy: bool, output: Option<&Path>) {
        let limits = ProtocolLimits::PRODUCTION;
        let messages = vec![
            Message::new(
                Role::System,
                vec![ContentBlock::Text(
                    BoundedText::new(system, limits).expect("system policy must remain bounded"),
                )],
                limits,
            )
            .expect("system policy must be a valid provider-neutral message"),
            Message::new(
                Role::User,
                vec![ContentBlock::Text(
                    BoundedText::new(TASK_SENTINEL.to_owned(), limits)
                        .expect("task sentinel must remain bounded"),
                )],
                limits,
            )
            .expect("task must be a valid provider-neutral message"),
        ];
        let encoded = encode_messages(&messages, limits).expect("messages must encode");
        let decoded = decode_messages(&encoded, limits).expect("messages must round-trip");
        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].role(), Role::System);
        assert_eq!(decoded[1].role(), Role::User);
        let system = decoded[0]
            .content()
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text(text) => Some(text.expose_for_wire()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        let user = decoded[1]
            .content()
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text(text) => Some(text.expose_for_wire()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(user, TASK_SENTINEL);
        assert_eq!(count(&user, DOMAIN_MARKER), 0);
        assert_eq!(count(&system, DOMAIN_MARKER), usize::from(expects_domain_policy));
        for section in DOMAIN_SECTIONS {
            assert_eq!(count(&system, section), usize::from(expects_domain_policy));
        }
        if let Some(output) = output {
            fs::write(output.join(format!("{role}.context")), encoded)
                .expect("write requested prompt-policy archive");
        }
    }

    #[test]
    fn domain_policy_has_exact_role_authority_and_canonical_archives() {
        let output = std::env::var_os("PERITUS_PROMPT_POLICY_ARCHIVE_OUT").map(|path| {
            let path = std::path::PathBuf::from(path);
            fs::create_dir_all(&path).expect("create requested prompt-policy archive directory");
            path
        });
        let output = output.as_deref();
        let automatic = crate::design::system_prompt(Some(Duration::from_secs(8317)));
        let marker = automatic.find(DOMAIN_MARKER).expect("automatic architect marker");
        let dynamic = automatic.find("# Dynamic run context").expect("dynamic run context");
        assert!(marker < dynamic, "stable policy must precede dynamic run context");
        archive("automatic-architect", automatic, true, output);
        archive(
            "plan",
            crate::execution::conversation_system(ConversationMode::Plan),
            true,
            output,
        );
        archive(
            "design",
            crate::execution::conversation_system(ConversationMode::Design),
            true,
            output,
        );
        archive(
            "chat",
            crate::execution::conversation_system(ConversationMode::Chat),
            false,
            output,
        );
        archive("developer", crate::engineering_workflow::developer("writer"), false, output);
        archive("fixer", crate::engineering_workflow::developer("fixer"), false, output);
        archive("reviewer", crate::engineering_workflow::reviewer(), false, output);
        archive(
            "conversation-reviewer",
            crate::execution::conversation_system(ConversationMode::Review),
            false,
            output,
        );
    }
}
