//! Reclaim completed disconnected owners before admitting another terminal.

use super::{TerminalBridgeError, TerminalRegistry};

impl TerminalRegistry {
    /// C2 owns native completion; losing a client or an output prefix does not lose that fact.
    /// Join outside the registry mutex because C4 may still need to publish its final receipt.
    pub(super) fn reap_completed(&self) -> Result<(), TerminalBridgeError> {
        let mut first_error = None;
        let retired = {
            let mut state = self.lock();
            let mut completed = Vec::new();
            for (process, bridge) in &mut state.processes {
                match bridge.observe_detached_completion() {
                    Ok(true) => completed.push(*process),
                    Ok(false) => {}
                    Err(error) => {
                        first_error.get_or_insert(error);
                    }
                }
            }
            completed
                .into_iter()
                .filter_map(|process| state.processes.remove(&process))
                .collect::<Vec<_>>()
        };
        for bridge in retired {
            if let Err(error) = bridge.join_owner() {
                first_error.get_or_insert(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }
}
