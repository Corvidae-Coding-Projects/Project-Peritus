//! Diagnostic limits must not crash the UI on a multibyte character boundary.
use super::*;

#[test]
fn bounded_notice_preserves_utf8_for_each_possible_boundary_offset() {
    let mut model = AppModel::new([3; 32]);
    let limit = model.limits.max_diagnostic_bytes();
    for offset in 0..4 {
        let text = format!("{}{}", "a".repeat(offset), "🦉".repeat(limit));
        model.notice(crate::model::NoticeLevel::Warning, text.clone());
        let shown = &model.notice.as_ref().expect("notice").text;
        assert!(shown.len() <= limit);
        assert!(text.starts_with(shown));
        assert!(limit - shown.len() < 4);
    }
}
