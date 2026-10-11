//! Small shared state enums for the workbench panel.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum WorkbenchMode {
    #[default]
    Sessions,
    Library,
    Queue,
    Brief,
    Compaction,
    Checkpoints,
    Permissions,
    Init,
    Memory,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum WorkbenchMemoryView {
    #[default]
    Current,
    History,
}

impl WorkbenchMemoryView {
    pub(super) const fn from_include_forgotten(include_forgotten: bool) -> Self {
        if include_forgotten { Self::History } else { Self::Current }
    }

    pub(super) const fn include_forgotten(self) -> bool {
        matches!(self, Self::History)
    }
}
