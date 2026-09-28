//! Process-local presentation retention across a launcher-owned daemon recovery.

use super::TuiConfig;
use crate::{action::Action, model::AppModel};

/// Bounded, process-local UI state retained while the launcher restores daemon readiness.
///
/// Reuse this value only with the same configuration. A different endpoint or workspace starts
/// a fresh interface; no draft or selected model is transferred between workspaces.
#[derive(Debug, Default)]
pub struct TuiState {
    saved: Option<(TuiConfig, Box<AppModel>)>,
}

impl TuiState {
    pub(super) fn take_model(&mut self, config: &TuiConfig, seed: [u8; 32]) -> Box<AppModel> {
        self.saved.take().filter(|(previous, _)| previous == config).map_or_else(
            || {
                let mut model = Box::new(AppModel::with_product(seed, config.product().cloned()));
                model.chat.run_id = config.product().and_then(super::ProductLaunchContext::run_id);
                model.chat.workbench.selected =
                    config.product().and_then(super::ProductLaunchContext::conversation);
                model.chat.workbench.open = model.chat.workbench.selected.is_some();
                model
            },
            |(_, model)| model,
        )
    }

    /// Reports a failed launcher navigation while keeping the original workspace and draft.
    /// The accepted fork remains saved; this error does not resubmit its creation command.
    pub fn conversation_open_failed(&mut self, detail: &str) {
        use std::fmt::Write as _;
        if let Some((_, model)) = &mut self.saved {
            model.view = crate::model::View::Conversation;
            model.chat.workbench.open = true;
            write!(
                model.chat.workbench.message,
                "\nCould not open the requested conversation: {detail}. Current workspace retained."
            )
            .expect("writing to String cannot fail");
        }
    }

    pub(super) fn retain(&mut self, config: TuiConfig, mut model: Box<AppModel>) {
        // Recover unacknowledged text without retaining transport-bound requests or authority.
        let _ = model.update(Action::Disconnected("restoring daemon readiness".to_owned()));
        model.terminal = None;
        model.prompts.clear();
        if let Some(product) = &mut model.product {
            product.confirmation = None;
        }
        self.saved = Some((config, model));
    }
}
