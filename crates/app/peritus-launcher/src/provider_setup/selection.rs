//! Provider-set and default selection parsing with recoverable prompts.

use peritus_product_state::ProviderKind;

use super::{ANTHROPIC_API, CLAUDE, CODEX, COMPATIBLE, GOOGLE_API, OPENAI_API};
use crate::{LauncherError, terminal::Terminal};

pub fn choose_provider_set(
    terminal: &mut Terminal<'_>,
    default: Vec<ProviderKind>,
    default_label: &str,
) -> Result<(Vec<ProviderKind>, bool), LauncherError> {
    let prompt = if default.is_empty() {
        "Providers (comma-separated numbers, or 0 for offline) [0]: ".to_owned()
    } else {
        format!("Providers (comma-separated numbers, or 0 for offline) [{default_label}]: ")
    };
    loop {
        let answer = terminal.prompt(&prompt)?;
        if answer.is_empty() {
            return Ok((default, true));
        }
        match parse_selection(&answer) {
            Ok(selection) => return Ok((selection, false)),
            Err(_) => terminal.line(
                "Choose displayed provider numbers separated by commas, or 0 for offline mode.",
            )?,
        }
    }
}

pub fn choose_default(
    terminal: &mut Terminal<'_>,
    enabled: &[ProviderKind],
) -> Result<Option<ProviderKind>, LauncherError> {
    match enabled {
        [] => Ok(None),
        [only] => Ok(Some(*only)),
        _ => {
            terminal.line("\nDefault provider for new runs:")?;
            for (index, kind) in enabled.iter().enumerate() {
                terminal.line(&format!("  {}. {}", index + 1, kind.label()))?;
            }
            loop {
                let answer = terminal.prompt("Default [1]: ")?;
                if answer.is_empty() || answer == "1" {
                    return Ok(enabled.first().copied());
                }
                if let Ok(index) = answer.parse::<usize>()
                    && let Some(kind) = index.checked_sub(1).and_then(|index| enabled.get(index))
                {
                    return Ok(Some(*kind));
                }
                terminal.line("Choose one of the displayed numbers.")?;
            }
        }
    }
}

pub fn choose_failover(
    terminal: &mut Terminal<'_>,
    enabled: &[ProviderKind],
    default: bool,
) -> Result<bool, LauncherError> {
    if enabled.len() < 2 {
        return Ok(false);
    }
    let suffix = if default { " [Y/n]: " } else { " [y/N]: " };
    terminal.confirm(
        &format!(
            "If one provider is temporarily unavailable, try another selected provider for that role?{suffix}"
        ),
        default,
    )
}

fn parse_selection(answer: &str) -> Result<Vec<ProviderKind>, LauncherError> {
    let normalized = answer.replace(' ', "");
    if normalized == "0" {
        return Ok(Vec::new());
    }
    let mut selected = Vec::new();
    for item in normalized.split(',') {
        let kind = match item {
            "1" => CODEX,
            "2" => CLAUDE,
            "3" => OPENAI_API,
            "4" => ANTHROPIC_API,
            "5" => GOOGLE_API,
            "6" => COMPATIBLE,
            _ => {
                return Err(LauncherError::Interaction(
                    "choose displayed provider numbers separated by commas, or 0 for offline mode"
                        .to_owned(),
                ));
            }
        };
        selected.push(kind);
    }
    selected.sort_unstable();
    selected.dedup();
    Ok(selected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_accepts_offline_single_and_multiple_routes() {
        assert_eq!(parse_selection("0").expect("offline"), Vec::new());
        assert_eq!(parse_selection("1").expect("codex"), vec![CODEX]);
        assert_eq!(parse_selection("2, 1,2").expect("both"), vec![CODEX, CLAUDE]);
    }

    #[test]
    fn selection_rejects_ambiguous_input() {
        assert!(parse_selection("all").is_err());
        assert!(parse_selection("0,1").is_err());
    }
}
