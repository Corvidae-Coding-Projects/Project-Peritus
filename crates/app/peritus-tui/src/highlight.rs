//! Local syntax presentation. Bundled grammars and themes are initialized once.

use std::sync::LazyLock;

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use syntect::{
    easy::HighlightLines,
    highlighting::{FontStyle, ThemeSet},
    parsing::SyntaxSet,
};

/// Syntax colors for a dark terminal, a light terminal, or the terminal's own foreground.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Theme {
    #[default]
    Dark,
    Light,
    Plain,
}

impl Theme {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "dark" => Some(Self::Dark),
            "light" => Some(Self::Light),
            "plain" | "none" => Some(Self::Plain),
            _ => None,
        }
    }

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
            Self::Plain => "plain",
        }
    }

    fn syntax_theme(self) -> Option<&'static syntect::highlighting::Theme> {
        let name = match self {
            Self::Dark => "base16-ocean.dark",
            Self::Light => "InspiredGitHub",
            Self::Plain => return None,
        };
        THEMES.themes.get(name)
    }
}

static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEMES: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

/// Highlight literal source without Markdown interpretation or terminal escape sequences.
/// Unknown languages and grammar errors retain their original display-safe content.
pub(crate) fn code(source: &str, language: &str, theme: Theme) -> Vec<Line<'static>> {
    let safe = crate::sanitize::sanitize_display_text(source);
    let Some(theme) = theme.syntax_theme() else {
        return literal(&safe);
    };
    let token = language.split_whitespace().next().unwrap_or_default();
    let token = token.split(',').next().unwrap_or_default();
    let token = match token.to_ascii_lowercase().as_str() {
        "typescript" | "ts" | "tsx" | "jsx" => "javascript",
        "shell" | "console" => "sh",
        _ => token,
    };
    let syntax = SYNTAXES.find_syntax_by_token(token).or_else(|| {
        std::path::Path::new(token)
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .and_then(|extension| SYNTAXES.find_syntax_by_extension(extension))
    });
    let Some(syntax) = syntax else {
        return literal(&safe);
    };
    let mut highlighter = HighlightLines::new(syntax, theme);
    // Keep a terminal newline in the parser's input so multiline lexical state is preserved.
    safe.split('\n')
        .map(|line| {
            let input = format!("{line}\n");
            let Ok(parts) = highlighter.highlight_line(&input, &SYNTAXES) else {
                return Line::raw(line.to_owned());
            };
            Line::from(
                parts
                    .into_iter()
                    .filter_map(|(style, text)| {
                        let text = text.trim_end_matches('\n');
                        if text.is_empty() {
                            return None;
                        }
                        let mut presentation = Style::default().fg(Color::Rgb(
                            style.foreground.r,
                            style.foreground.g,
                            style.foreground.b,
                        ));
                        if style.font_style.contains(FontStyle::BOLD) {
                            presentation = presentation.add_modifier(Modifier::BOLD);
                        }
                        if style.font_style.contains(FontStyle::ITALIC) {
                            presentation = presentation.add_modifier(Modifier::ITALIC);
                        }
                        if style.font_style.contains(FontStyle::UNDERLINE) {
                            presentation = presentation.add_modifier(Modifier::UNDERLINED);
                        }
                        Some(Span::styled(text.to_owned(), presentation))
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect()
}

/// Highlight the standard diff grammar while preserving every hunk and literal line.
pub(crate) fn diff(source: &str, theme: Theme) -> Vec<Line<'static>> {
    code(source, "diff", theme)
}

fn literal(source: &str) -> Vec<Line<'static>> {
    source.split('\n').map(|line| Line::raw(line.to_owned())).collect()
}
