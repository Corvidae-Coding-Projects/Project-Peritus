//! Exactly one selection overlay can own conversation input at a time.

use super::ChatUi;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Picker {
    Model,
    Effort,
}

impl ChatUi {
    pub(crate) fn model_picker(&self) -> bool {
        self.picker == Some(Picker::Model)
    }
    pub(crate) fn effort_picker(&self) -> bool {
        self.picker == Some(Picker::Effort)
    }
    pub(crate) const fn show_model_picker(&mut self) {
        self.picker = Some(Picker::Model);
    }
    pub(crate) const fn show_effort_picker(&mut self) {
        self.picker = Some(Picker::Effort);
    }
    pub(crate) fn close_model_picker(&mut self) {
        if self.model_picker() {
            self.picker = None;
        }
    }
    pub(crate) fn close_effort_picker(&mut self) {
        if self.effort_picker() {
            self.picker = None;
        }
    }
}
