//! Structural validation for inert initialization paths and command tokens.

pub(super) fn valid_relative_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 4096
        && !path.starts_with('/')
        && !path.ends_with('/')
        && !path.bytes().any(|byte| byte < 0x20 || byte == 0x7f || matches!(byte, b'\\' | b':'))
        && path
            .split('/')
            .all(|component| !component.is_empty() && component != "." && component != "..")
}

pub(super) fn valid_command_part(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && !value.chars().any(char::is_control)
        && !value.contains('`')
}
