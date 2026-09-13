//! Stable message-family registry.

/// G0-owned immutable actor/conversation/workspace artifact claim event.
pub const ARTIFACT_SCOPE_CLAIM_FAMILY: u16 = 3500;
/// G0-owned artifact upload acceptance event; preserves the existing publication allocation.
pub const ARTIFACT_UPLOAD_ACCEPTED_FAMILY: u16 = 65_000;

/// One immutable canonical frame family.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MessageFamily {
    /// Nonzero frame-header family tag.
    pub tag: u16,
    /// Stable kebab-case family name.
    pub name: &'static str,
    /// Current nonzero schema version.
    pub schema_version: u16,
    /// Nonempty, strictly increasing schema versions accepted by transport consumers.
    pub supported_schema_versions: &'static [u16],
    /// Whether decoded values remain inert records rather than reconstructible requests.
    pub inert_only: bool,
}

/// Stable semantic role of one B3 message family.
///
/// The role lets transport contracts distinguish exact command and event frames without copying
/// B3 data-transfer objects or maintaining a second family registry.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MessageRole {
    /// A command payload accepted by its owning domain reducer.
    Command,
    /// The canonical B0 command envelope.
    CommandEnvelope,
    /// An immutable semantic event or observation.
    Event,
    /// A complete replayable aggregate state.
    State,
    /// Another canonical record, definition, receipt, error, or phase value.
    Record,
}

impl MessageFamily {
    /// Returns whether this family accepts the requested schema version.
    #[must_use]
    pub const fn supports(self, version: u16) -> bool {
        let mut index = 0;
        while index < self.supported_schema_versions.len() {
            if self.supported_schema_versions[index] == version {
                return true;
            }
            index += 1;
        }
        false
    }

    /// Returns the stable semantic role of this registered family.
    #[must_use]
    pub const fn role(self) -> MessageRole {
        match self.tag {
            1 | 10 | 40 | 50 | 53 | 70 | 73 | 76 | 79 | 82 | 85 | 88 | 91 => MessageRole::Command,
            2 => MessageRole::CommandEnvelope,
            3
            | 41
            | 51
            | 54
            | 60
            | 71
            | 74
            | 77
            | 80
            | 83
            | 86
            | 89
            | 92
            | 94
            | ARTIFACT_SCOPE_CLAIM_FAMILY
            | ARTIFACT_UPLOAD_ACCEPTED_FAMILY => MessageRole::Event,
            12 | 13 | 42 | 52 | 55 | 72 | 75 | 78 | 81 | 84 | 87 | 90 | 93 => MessageRole::State,
            _ => MessageRole::Record,
        }
    }
}

mod families;

pub use families::FAMILIES;
