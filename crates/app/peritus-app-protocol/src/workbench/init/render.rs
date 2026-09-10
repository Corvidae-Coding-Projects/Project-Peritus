//! Deterministic managed-section content and whole-file review rendering.

use super::{AppProtocolError, INIT_SECTION_END, INIT_SECTION_START, InitCommand, invalid};

pub(super) fn managed_content(
    original: Option<&str>,
    commands: &[InitCommand],
) -> Result<String, AppProtocolError> {
    let section = managed_section(commands);
    let Some(original) = original else {
        return Ok(format!("{section}\n"));
    };
    let starts = original.match_indices(INIT_SECTION_START).collect::<Vec<_>>();
    let ends = original.match_indices(INIT_SECTION_END).collect::<Vec<_>>();
    if starts.is_empty() && ends.is_empty() {
        let separator = if original.is_empty() || original.ends_with("\n\n") {
            ""
        } else if original.ends_with('\n') {
            "\n"
        } else {
            "\n\n"
        };
        return Ok(format!("{original}{separator}{section}\n"));
    }
    if starts.len() != 1 || ends.len() != 1 || starts[0].0 >= ends[0].0 {
        return Err(invalid());
    }
    let end = ends[0].0.checked_add(INIT_SECTION_END.len()).ok_or_else(invalid)?;
    let mut proposed = String::with_capacity(original.len().saturating_add(section.len()));
    proposed.push_str(&original[..starts[0].0]);
    proposed.push_str(&section);
    proposed.push_str(&original[end..]);
    Ok(proposed)
}

fn managed_section(commands: &[InitCommand]) -> String {
    use std::fmt::Write as _;

    let mut section = format!(
        "{INIT_SECTION_START}\n## Peritus-discovered project controls\n\n\
         These commands were discovered from selected local project files. They are unverified and \
         require separate explicit trust before execution. Accepting this section does not run them."
    );
    if commands.is_empty() {
        section.push_str(
            "\n\nNo build, test, lint, or launch command was discovered in the selected files.",
        );
    } else {
        for command in commands {
            write!(
                section,
                "\n\n- {} (unverified, from `{}`): `{}`",
                command.kind().as_str(),
                command.source(),
                command.executable()
            )
            .expect("writing into String cannot fail");
            for argument in command.arguments() {
                write!(section, " {argument}").expect("writing into String cannot fail");
            }
            section.push('`');
        }
    }
    write!(section, "\n\n{INIT_SECTION_END}").expect("writing into String cannot fail");
    section
}

pub(super) fn render_exact_diff(path: &str, original: Option<&str>, proposed: &str) -> String {
    let original = original.unwrap_or("");
    let old_lines = line_count(original);
    let new_lines = line_count(proposed);
    let old_start = usize::from(old_lines > 0);
    let new_start = usize::from(new_lines > 0);
    let mut diff = format!(
        "--- a/{path}\n+++ b/{path}\n@@ -{old_start},{old_lines} +{new_start},{new_lines} @@\n"
    );
    append_diff_lines(&mut diff, '-', original);
    append_diff_lines(&mut diff, '+', proposed);
    diff
}

fn line_count(value: &str) -> usize {
    if value.is_empty() {
        0
    } else {
        value.bytes().filter(|byte| *byte == b'\n').count() + usize::from(!value.ends_with('\n'))
    }
}

fn append_diff_lines(output: &mut String, prefix: char, value: &str) {
    for line in value.split_inclusive('\n') {
        output.push(prefix);
        output.push_str(line);
        if !line.ends_with('\n') {
            output.push('\n');
            output.push_str("\\ No newline at end of file\n");
        }
    }
}
