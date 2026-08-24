//! Durable process-tree identity observations.

/// Exact root identity used to guard recovery against PID reuse.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProcessTreeIdentity {
    root_pid: u32,
    start_token: Option<u64>,
    process_group: Option<u32>,
    complete_containment: bool,
}

impl ProcessTreeIdentity {
    /// Creates one observed native process-tree identity.
    ///
    /// This is an observation value, not execution authority. C2 validates and persists the value
    /// before it contributes to lifecycle or recovery decisions.
    #[must_use]
    pub const fn new(
        root_pid: u32,
        start_token: Option<u64>,
        process_group: Option<u32>,
        complete_containment: bool,
    ) -> Self {
        Self { root_pid, start_token, process_group, complete_containment }
    }

    /// Returns the root operating-system process identifier.
    #[must_use]
    pub const fn root_pid(self) -> u32 {
        self.root_pid
    }
    /// Returns the backend-specific process birth token when observable.
    #[must_use]
    pub const fn start_token(self) -> Option<u64> {
        self.start_token
    }
    /// Returns the owned Unix process group/session leader when available.
    #[must_use]
    pub const fn process_group(self) -> Option<u32> {
        self.process_group
    }
    /// Returns whether complete descendant containment is available.
    #[must_use]
    pub const fn complete_containment(self) -> bool {
        self.complete_containment
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn current_start_token(pid: u32) -> Option<u64> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let close = text.rfind(')')?;
    let fields: Vec<&str> = text.get(close + 2..)?.split_ascii_whitespace().collect();
    fields.get(19)?.parse().ok()
}

#[cfg(not(target_os = "linux"))]
pub(crate) const fn current_start_token(_pid: u32) -> Option<u64> {
    None
}
