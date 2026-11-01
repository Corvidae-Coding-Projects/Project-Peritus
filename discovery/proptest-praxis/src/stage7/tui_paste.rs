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
    pub cursor_scalar: u16,
    pub paste: String,
    #[serde(default)]
    pub mode: EditorMode,
    #[serde(default)]
    pub following: Vec<Operation>,
    #[serde(default)]
    pub small_limits: Option<(u16, u16)>,
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
    Move(u16),
}

fn text(chars: Vec<char>) -> String {
    chars.into_iter().collect()
}

fn modes() -> impl Strategy<Value = EditorMode> {
    prop::sample::select(vec![
        EditorMode::ProductMessage,
        EditorMode::ReviewFeedback,
        EditorMode::ProcessId,
        EditorMode::ApprovalSignature,
        EditorMode::PromptAnswer,
    ])
}

fn limits(small: Option<(u16, u16)>) -> Result<AppProtocolLimits, String> {
    let Some((frame, string)) = small else { return Ok(AppProtocolLimits::PRODUCTION) };
    let payload = usize::from(frame)
        .checked_sub(peritus_codec::HEADER_LEN)
        .ok_or("frame smaller than protocol header")?;
    let codec = peritus_codec::CodecLimits::new(
        usize::from(frame),
        payload,
        16,
        usize::from(string),
        payload,
        8,
    );
    AppProtocolLimits::new(codec, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1).map_err(|e| e.to_string())
}

pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    let alphabet = prop::sample::select(vec![
        'a', '/', ' ', '\n', 'é', '界', '🙂', '\u{7}', '\t', '\r', '\u{85}', '\u{301}',
    ]);
    let small = (0_u16..=2, 1_u16..=64).prop_map(|(padding, string)| {
        (u16::try_from(peritus_codec::HEADER_LEN).unwrap() + 128 + padding, string)
    });
    let following = prop::collection::vec(
        prop_oneof![
            4 => alphabet.clone().prop_map(Operation::Type),
            3 => prop::collection::vec(alphabet.clone(), 0..24)
                .prop_map(|chars| Operation::Paste(text(chars))),
            2 => prop_oneof![Just(0_u16), Just(u16::MAX), 0_u16..=48].prop_map(Operation::Move),
        ],
        0..=max_steps.clamp(1, 64),
    );
    let general = (
        prop::collection::vec(alphabet.clone(), 0..32),
        prop_oneof![Just(0_u16), Just(u16::MAX), 0_u16..=48],
        prop::collection::vec(alphabet, 0..96),
        modes(),
        following,
        prop::option::of(small.clone()),
    )
        .prop_map(|(initial, cursor_scalar, paste, mode, following, small_limits)| Case {
            initial: text(initial),
            cursor_scalar,
            paste: text(paste),
            mode,
            following,
            small_limits,
        });
    // Put every editor at its own byte boundary, then continue after rejection.
    // ASCII padding plus a multibyte tail separates byte and character ceilings.
    let boundary = (
        modes(),
        small,
        0_usize..=4,
        0_usize..=4,
        prop::sample::select(vec!["x", "é", "界", "🙂"]),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(|(mode, small, room, extra, tail, slash, at_end)| {
            let small_limits = Some(small);
            let maximum = mode.maximum(limits(small_limits).unwrap());
            let remaining = room.min(maximum);
            let mut initial = "x".repeat(maximum - remaining);
            if slash && !initial.is_empty() {
                initial.replace_range(..1, "/");
            }
            let paste = format!("{}{}", "x".repeat(extra), tail);
            Case {
                initial,
                cursor_scalar: if at_end { u16::MAX } else { 0 },
                paste,
                mode,
                small_limits,
                following: vec![
                    Operation::Move(u16::MAX),
                    Operation::Type('a'),
                    Operation::Paste("\u{7}\r".into()),
                    Operation::Paste("🙂".into()),
                    Operation::Move(0),
                    Operation::Paste("/".into()),
                ],
            }
        });
    prop_oneof![4 => general, 3 => boundary].boxed()
}

pub fn calibration() -> Case {
    Case {
        initial: String::new(),
        cursor_scalar: 0,
        paste: "x".repeat(MAX_WORKBENCH_INPUT_BYTES),
        mode: EditorMode::ProductMessage,
        following: Vec::new(),
        small_limits: None,
    }
}

pub fn examples() -> Vec<Case> {
    let mut cases = vec![
        calibration(),
        Case {
            initial: String::new(),
            cursor_scalar: 0,
            paste: "Shared https://doc.qt.io for reading.".to_owned(),
            mode: EditorMode::ProductMessage,
            following: vec![Operation::Type('!'), Operation::Paste(" 🙂".into())],
            small_limits: None,
        },
        Case {
            initial: "é🙂".to_owned(),
            cursor_scalar: 1,
            paste: "界\u{7}x\n".to_owned(),
            mode: EditorMode::ProductMessage,
            following: Vec::new(),
            small_limits: None,
        },
        Case {
            initial: "/runs".to_owned(),
            cursor_scalar: 5,
            paste: "x".repeat(MAX_WORKBENCH_INPUT_BYTES + 1),
            mode: EditorMode::ProductMessage,
            following: Vec::new(),
            small_limits: None,
        },
        Case {
            initial: "draft".to_owned(),
            cursor_scalar: 2,
            paste: "x".repeat(MAX_WORKBENCH_INPUT_BYTES + 1),
            mode: EditorMode::ProductMessage,
            following: Vec::new(),
            small_limits: None,
        },
        Case {
            initial: "/review".to_owned(),
            cursor_scalar: 7,
            paste: " response".to_owned(),
            mode: EditorMode::ReviewFeedback,
            following: Vec::new(),
            small_limits: None,
        },
        Case {
            initial: "draft".to_owned(),
            cursor_scalar: 5,
            paste: "x".repeat(252),
            mode: EditorMode::ProcessId,
            following: Vec::new(),
            small_limits: None,
        },
        Case {
            initial: String::new(),
            cursor_scalar: 0,
            paste: "signature".to_owned(),
            mode: EditorMode::ApprovalSignature,
            following: Vec::new(),
            small_limits: None,
        },
        Case {
            initial: String::new(),
            cursor_scalar: 0,
            paste: "answer".to_owned(),
            mode: EditorMode::PromptAnswer,
            following: Vec::new(),
            small_limits: None,
        },
    ];
    for mode in [
        EditorMode::ProductMessage,
        EditorMode::ReviewFeedback,
        EditorMode::ProcessId,
        EditorMode::ApprovalSignature,
        EditorMode::PromptAnswer,
    ] {
        for padding in 0..=2 {
            let small_limits =
                Some((u16::try_from(peritus_codec::HEADER_LEN).unwrap() + 128 + padding, 8));
            let maximum = mode.maximum(limits(small_limits).unwrap());
            for remaining in 0..=4 {
                cases.push(Case {
                    initial: format!("/{}", "x".repeat(maximum - remaining - 1)),
                    cursor_scalar: u16::MAX,
                    paste: "🙂".into(),
                    mode,
                    small_limits,
                    following: vec![
                        Operation::Move(1),
                        Operation::Type('é'),
                        Operation::Paste("\r\0".into()),
                        Operation::Type('a'),
                        Operation::Move(0),
                        Operation::Paste("/".into()),
                    ],
                });
            }
        }
    }
    cases
}

pub fn check(case: &Case) -> Result<Metrics, String> {
    if case.initial.len() > MAX_WORKBENCH_INPUT_BYTES + 128
        || case.paste.len() > MAX_WORKBENCH_INPUT_BYTES + 128
        || case.following.len() > 64
        || case.following.iter().any(|operation| match operation {
            Operation::Paste(text) => text.len() > 128,
            Operation::Type(_) | Operation::Move(_) => false,
        })
    {
        return Err("paste fixture bound exceeded".to_owned());
    }
    let scalar = usize::from(case.cursor_scalar).min(case.initial.chars().count());
    let cursor = case.initial.char_indices().nth(scalar).map_or(case.initial.len(), |(n, _)| n);
    let limits = limits(case.small_limits)?;
    let maximum = case.mode.maximum(limits);
    let mut actual = AppModel {
        editor: Some(Editor {
            kind: case.mode.kind(),
            buffer: case.initial.clone(),
            cursor,
            pasted_command: false,
        }),
        limits,
        warning: None,
    };
    let mut metrics = Metrics::new();
    // A scalar vector and scalar cursor are independent of production byte insertion.
    let mut characters = case.initial.chars().collect::<Vec<_>>();
    let mut scalar_cursor = scalar;
    let mut expected_command = false;
    let mut saw_rejection = false;
    for (index, operation) in std::iter::once(Operation::Paste(case.paste.clone()))
        .chain(case.following.iter().cloned())
        .enumerate()
    {
        if let Operation::Move(position) = operation {
            scalar_cursor = usize::from(position).min(characters.len());
            actual.editor.as_mut().ok_or("missing editor")?.cursor =
                characters[..scalar_cursor].iter().map(|ch| ch.len_utf8()).sum();
            *metrics.entry("cursor_repositions".into()).or_default() += 1;
            continue;
        }
        let (text, is_paste) = match &operation {
            Operation::Type(character) => (character.to_string(), false),
            Operation::Paste(text) => (text.clone(), true),
            Operation::Move(_) => unreachable!("handled above"),
        };
        let sanitized = text
            .chars()
            .filter(|ch| !ch.is_control() || matches!(ch, '\n' | '\t'))
            .collect::<Vec<_>>();
        let existing_bytes: usize = characters.iter().map(|ch| ch.len_utf8()).sum();
        let inserted_bytes: usize = sanitized.iter().map(|ch| ch.len_utf8()).sum();
        let rejected = existing_bytes + inserted_bytes > maximum;
        if !rejected {
            let inserted = sanitized.len();
            characters.splice(scalar_cursor..scalar_cursor, sanitized);
            scalar_cursor += inserted;
            if is_paste
                && matches!(case.mode, EditorMode::ProductMessage)
                && characters.iter().find(|ch| !ch.is_whitespace()) == Some(&'/')
            {
                expected_command = true;
            }
        }
        let expected: String = characters.iter().collect();
        let expected_cursor: usize =
            characters[..scalar_cursor].iter().map(|ch| ch.len_utf8()).sum();
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
            saw_rejection = true;
            *metrics.entry(format!("rejected_{:?}", case.mode)).or_default() += 1;
            *metrics.entry("rejected_oversize".into()).or_default() += 1;
            if expected.trim_start().starts_with('/') {
                *metrics.entry("rejected_command_drafts".into()).or_default() += 1;
            }
        } else {
            if saw_rejection {
                *metrics.entry("accepted_after_rejection".into()).or_default() += 1;
            }
            if expected.len() == maximum {
                *metrics.entry(format!("exact_byte_cap_{:?}", case.mode)).or_default() += 1;
            }
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
