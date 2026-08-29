//! Task-level completion summary conservation.

use std::{fmt::Write as _, path::PathBuf};

use super::ProductDeliveryScope;

pub(super) fn completion_summary(
    task: &str,
    writer: &str,
    fixes: &[String],
    changed_paths: &[PathBuf],
    command_count: usize,
    delivery_scope: ProductDeliveryScope,
) -> String {
    let mut summary = format!(
        "Completed the requested task: {}\n\nImplementation: {}",
        task.trim(),
        writer.trim(),
    );
    if !fixes.is_empty() {
        summary.push_str("\n\nVerified fixes:");
        for fix in fixes {
            summary.push_str("\n- ");
            summary.push_str(fix.trim());
        }
    }
    if changed_paths.is_empty() && delivery_scope.allows_external_effects() {
        let _ = write!(
            summary,
            "\n\nDeliverable: caller-authorized external effects; {command_count} retained effect and verification command(s) passed.",
        );
    } else {
        let _ = write!(
            summary,
            "\n\nDeliverable: {} changed file(s); {command_count} exact-target acceptance command(s) passed.",
            changed_paths.len(),
        );
    }
    crate::bundle::limit_text(&summary, 256 * 1024)
}
