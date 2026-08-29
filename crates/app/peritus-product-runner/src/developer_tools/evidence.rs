//! Bounded developer-command observations carried into independent review.

use std::collections::VecDeque;

use serde_json::Value;

const MAX_RECORDS: usize = 32;
const MAX_RECORD_BYTES: usize = 16 * 1024;
const MAX_RENDERED_BYTES: usize = 128 * 1024;

#[derive(Default)]
pub(super) struct CommandEvidence {
    records: VecDeque<String>,
    bytes: usize,
}

impl CommandEvidence {
    pub(super) fn record(&mut self, arguments: &Value, result: &Value) {
        let request = serde_json::to_string(arguments).unwrap_or_else(|_| "<invalid JSON>".into());
        let result = serde_json::to_string(result).unwrap_or_else(|_| "<invalid JSON>".into());
        let record = format!(
            "request: {}\nresult: {}",
            preview(&request, MAX_RECORD_BYTES / 4),
            preview(&result, MAX_RECORD_BYTES * 3 / 4),
        );
        self.bytes = self.bytes.saturating_add(record.len());
        self.records.push_back(record);
        while self.records.len() > MAX_RECORDS || self.bytes > MAX_RENDERED_BYTES {
            let Some(removed) = self.records.pop_front() else {
                break;
            };
            self.bytes = self.bytes.saturating_sub(removed.len());
        }
    }

    pub(super) fn render(&self) -> String {
        self.records
            .iter()
            .enumerate()
            .map(|(index, record)| format!("[Developer command {}]\n{record}", index + 1))
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

pub(super) fn merge_rendered(retained: &mut String, incoming: &str) {
    if incoming.is_empty() {
        return;
    }
    if !retained.is_empty() {
        retained.push_str("\n\n");
    }
    retained.push_str(incoming);
    if retained.len() > MAX_RENDERED_BYTES {
        let start = retained.len() - MAX_RENDERED_BYTES;
        let boundary = retained.ceil_char_boundary(start);
        *retained =
            format!("[earlier developer command evidence omitted]\n{}", &retained[boundary..]);
    }
}

fn preview(value: &str, maximum: usize) -> String {
    if value.len() <= maximum {
        return value.to_owned();
    }
    let boundary = value.floor_char_boundary(maximum);
    format!("{}...[truncated]", &value[..boundary])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_retains_recent_bounded_command_observations() {
        let mut evidence = CommandEvidence::default();
        for index in 0..40 {
            evidence.record(
                &serde_json::json!({"program":"python","args":[format!("check-{index}.py")]}),
                &serde_json::json!({"success":true,"stdout":"verified"}),
            );
        }

        let rendered = evidence.render();
        assert!(!rendered.contains("check-0.py"));
        assert!(rendered.contains("check-39.py"));
        assert!(rendered.len() <= MAX_RENDERED_BYTES);
    }
}
