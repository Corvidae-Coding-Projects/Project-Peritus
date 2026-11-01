//! Model for bounded modal-editor paste, using the TUI's checked-in method body.

use crate::{
    Metrics,
    model::{AppModel, Editor, EditorKind, paste, type_text},
};
use peritus_app_protocol::{AppProtocolLimits, MAX_WORKBENCH_INPUT_BYTES};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Case {
    pub initial: String,
    pub cursor_scalar: u8,
    pub paste: String,
    #[serde(default)]
    pub mode: EditorMode,
    #[serde(default)]
    pub following: Vec<Operation>,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditorMode {
    #[default]
    ProductMessage,
    ReviewFeedback,
    ProcessId,
    ApprovalSignature,
    PromptAnswer,
}

impl EditorMode {
    fn kind(self) -> EditorKind {
        match self {
            Self::ProductMessage => EditorKind::ProductMessage(()),
            Self::ReviewFeedback => EditorKind::ReviewFeedback(()),
            Self::ProcessId => EditorKind::ProcessId,
            Self::ApprovalSignature => EditorKind::ApprovalSignature(()),
            Self::PromptAnswer => EditorKind::PromptAnswer(()),
        }
    }

    fn maximum(self, limits: AppProtocolLimits) -> usize {
        match self {
            Self::ProductMessage | Self::ReviewFeedback => MAX_WORKBENCH_INPUT_BYTES,
            Self::ProcessId => 256,
            Self::ApprovalSignature => {
                let frame = limits.codec().max_frame_bytes as u128;
                usize::try_from(frame.div_ceil(3) * 4).expect("production frame limit fits usize")
            }
            Self::PromptAnswer => limits.codec().max_string_bytes,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", content = "text", rename_all = "snake_case")]
pub enum Operation {
    Type(char),
    Paste(String),
}

fn text(chars: Vec<char>) -> String {
    chars.into_iter().collect()
}

pub fn strategy(_max_steps: usize) -> BoxedStrategy<Case> {
    let alphabet =
        prop::sample::select(vec!['a', 'b', '/', ' ', '\n', 'é', '界', '🙂', '\u{7}', '\t']);
    let following = prop::collection::vec(
        prop_oneof![
            4 => prop::sample::select(vec!['a', 'b', '/', ' ', 'é', '界', '🙂']).prop_map(Operation::Type),
            2 => prop::collection::vec(
                prop::sample::select(vec!['a', 'b', '/', ' ', '\n', 'é', '界', '🙂', '\u{7}', '\t']),
                0..24,
            ).prop_map(|chars| Operation::Paste(text(chars))),
        ],
        0..10,
    );
    let general = (
        prop::collection::vec(alphabet.clone(), 0..32),
        any::<u8>(),
        prop::collection::vec(alphabet, 0..96),
        prop::sample::select(vec![
            EditorMode::ProductMessage,
            EditorMode::ReviewFeedback,
            EditorMode::ProcessId,
            EditorMode::ApprovalSignature,
            EditorMode::PromptAnswer,
        ]),
        following,
    )
        .prop_map(|(initial, cursor_scalar, paste, mode, following)| Case {
            initial: text(initial),
            cursor_scalar,
            paste: text(paste),
            mode,
            following,
        });
    let boundary = (0_usize..=3, 0_usize..=4, any::<bool>()).prop_map(|(side, initial, slash)| {
        let initial = if slash { "/runs".to_owned() } else { "é".repeat(initial) };
        let paste = "x"
            .repeat(MAX_WORKBENCH_INPUT_BYTES.saturating_add(side).saturating_sub(initial.len()));
        Case {
            initial,
            cursor_scalar: 255,
            paste,
            mode: EditorMode::ProductMessage,
            following: Vec::new(),
        }
    });
    prop_oneof![5 => general, 2 => boundary].boxed()
}

pub fn calibration() -> Case {
    Case {
        initial: String::new(),
        cursor_scalar: 0,
        paste: "x".repeat(MAX_WORKBENCH_INPUT_BYTES),
        mode: EditorMode::ProductMessage,
        following: Vec::new(),
    }
}

pub fn examples() -> Vec<Case> {
    vec![
        calibration(),
        Case {
            initial: String::new(),
            cursor_scalar: 0,
            paste: "Shared https://doc.qt.io for reading.".to_owned(),
            mode: EditorMode::ProductMessage,
            following: vec![Operation::Type('!'), Operation::Paste(" 🙂".into())],
        },
        Case {
            initial: "é🙂".to_owned(),
            cursor_scalar: 1,
            paste: "界\u{7}x\n".to_owned(),
            mode: EditorMode::ProductMessage,
            following: Vec::new(),
        },
        Case {
            initial: "/runs".to_owned(),
            cursor_scalar: 5,
            paste: "x".repeat(MAX_WORKBENCH_INPUT_BYTES + 1),
            mode: EditorMode::ProductMessage,
            following: Vec::new(),
        },
        Case {
            initial: "draft".to_owned(),
            cursor_scalar: 2,
            paste: "x".repeat(MAX_WORKBENCH_INPUT_BYTES + 1),
            mode: EditorMode::ProductMessage,
            following: Vec::new(),
        },
        Case {
            initial: "/review".to_owned(),
            cursor_scalar: 7,
            paste: " response".to_owned(),
            mode: EditorMode::ReviewFeedback,
            following: Vec::new(),
        },
        Case {
            initial: "draft".to_owned(),
            cursor_scalar: 5,
            paste: "x".repeat(252),
            mode: EditorMode::ProcessId,
            following: Vec::new(),
        },
        Case {
            initial: String::new(),
            cursor_scalar: 0,
            paste: "signature".to_owned(),
            mode: EditorMode::ApprovalSignature,
            following: Vec::new(),
        },
        Case {
            initial: String::new(),
            cursor_scalar: 0,
            paste: "answer".to_owned(),
            mode: EditorMode::PromptAnswer,
            following: Vec::new(),
        },
    ]
}

pub fn check(case: &Case) -> Result<Metrics, String> {
    if case.initial.len() > 128
        || case.paste.len() > MAX_WORKBENCH_INPUT_BYTES + 128
        || case.following.len() > 10
        || case.following.iter().any(|operation| match operation {
            Operation::Paste(text) => text.len() > 128,
            Operation::Type(_) => false,
        })
    {
        return Err("paste fixture bound exceeded".to_owned());
    }
    let scalar = usize::from(case.cursor_scalar).min(case.initial.chars().count());
    let cursor = case.initial.char_indices().nth(scalar).map_or(case.initial.len(), |(n, _)| n);
    let maximum = case.mode.maximum(AppProtocolLimits::PRODUCTION);
    let mut actual = AppModel {
        editor: Some(Editor {
            kind: case.mode.kind(),
            buffer: case.initial.clone(),
            cursor,
            pasted_command: false,
        }),
        limits: AppProtocolLimits::PRODUCTION,
        warning: None,
    };
    let mut metrics = Metrics::new();
    let mut expected = case.initial.clone();
    let mut expected_cursor = cursor;
    let mut expected_command = false;
    for (index, operation) in std::iter::once(Operation::Paste(case.paste.clone()))
        .chain(case.following.iter().cloned())
        .enumerate()
    {
        let (text, is_paste) = match &operation {
            Operation::Type(character) => (character.to_string(), false),
            Operation::Paste(text) => (text.clone(), true),
        };
        let sanitized = text
            .chars()
            .filter(|ch| !ch.is_control() || matches!(ch, '\n' | '\t'))
            .take(maximum.saturating_add(1))
            .collect::<String>();
        let rejected = expected.len().saturating_add(sanitized.len()) > maximum;
        if !rejected {
            expected.insert_str(expected_cursor, &sanitized);
            expected_cursor += sanitized.len();
            if is_paste
                && matches!(case.mode, EditorMode::ProductMessage)
                && expected.trim_start().starts_with('/')
            {
                expected_command = true;
            }
        }
        let previous_warning = actual.warning.clone();
        if is_paste {
            paste(&mut actual, &text)
        } else {
            type_text(&mut actual, &text)
        }
        let observed = actual.editor.as_ref().ok_or("paste closed editor")?;
        if observed.buffer != expected
            || observed.cursor != expected_cursor
            || !observed.buffer.is_char_boundary(observed.cursor)
        {
            return Err(format!(
                "operation {index} changed draft/cursor: expected {expected:?}/{expected_cursor}, observed {:?}/{}",
                observed.buffer, observed.cursor
            ));
        }
        if rejected
            && actual.warning.as_deref().is_none_or(|value| !value.contains("draft retained"))
        {
            return Err(format!(
                "operation {index}: oversize insertion did not report retained draft"
            ));
        }
        if !rejected && actual.warning != previous_warning {
            return Err(format!("operation {index}: accepted insertion reported a warning"));
        }
        if observed.pasted_command != expected_command {
            return Err(format!(
                "operation {index}: paste command provenance changed on {} insertion: expected {expected_command}, observed {}",
                if rejected { "rejected" } else { "accepted" },
                observed.pasted_command
            ));
        }
        if is_paste {
            *metrics.entry("paste_operations".into()).or_default() += 1;
        } else {
            *metrics.entry("typed_characters".into()).or_default() += 1;
        }
        if rejected {
            *metrics.entry("rejected_oversize".into()).or_default() += 1;
            if expected.trim_start().starts_with('/') {
                *metrics.entry("rejected_command_drafts".into()).or_default() += 1;
            }
        } else {
            *metrics.entry("accepted".into()).or_default() += 1;
        }
        if text.chars().any(|ch| ch.len_utf8() > 1) {
            *metrics.entry("unicode_insertions".into()).or_default() += 1;
        }
    }
    *metrics.entry("paste_cases".into()).or_default() += 1;
    *metrics.entry(format!("editor_{:?}", case.mode)).or_default() += 1;
    if case.paste == "Shared https://doc.qt.io for reading." {
        *metrics.entry("qt_short_seed".into()).or_default() += 1;
    }
    Ok(metrics)
}
