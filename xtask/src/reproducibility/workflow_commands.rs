use std::collections::BTreeSet;
use std::path::Path;

use super::workflow_command_syntax::{
    is_assignment, is_control_word, is_non_resolving, is_opaque, option_takes_value,
};

const PRE_CARGO_AUTHORITY: &str = "6ca5f56d2ab12e93f155d684b33f4a86c2f877b8";
pub(super) const WORKSPACE_TEST_ARGS: &[&str] = &[
    "test",
    "--workspace",
    "--all-targets",
    "--all-features",
    "--locked",
    "--",
    "--test-threads=1",
];

#[derive(Clone, Copy)]
pub(super) struct CommandPolicy {
    locked_xtask_alias: bool,
}

impl CommandPolicy {
    pub(super) const fn new(locked_xtask_alias: bool) -> Self {
        Self { locked_xtask_alias }
    }

    pub(super) const fn permits_xtask(self) -> bool {
        self.locked_xtask_alias
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum ScriptIssue {
    BackgroundExecution,
    DynamicShell,
    FailureMasking,
    NestedShell,
    ShellControlFlow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Connector {
    End,
    Sequence,
    And,
    Pipe,
    Or,
    Background,
}

#[derive(Debug)]
pub(super) struct Command {
    words: Vec<String>,
    executable: Option<usize>,
    connector: Connector,
}

impl Command {
    pub(super) fn executable_word(&self) -> Option<&str> {
        self.executable.and_then(|index| self.words.get(index)).map(String::as_str)
    }

    pub(super) fn is_dependency_resolving(&self) -> bool {
        self.cargo_subcommand().is_some_and(|subcommand| !is_non_resolving(subcommand))
    }

    pub(super) fn is_xtask(&self) -> bool {
        self.cargo_subcommand() == Some("xtask")
    }

    pub(super) fn has_locked_input(&self) -> bool {
        self.cargo_arguments().iter().any(|argument| argument == "--locked")
    }

    pub(super) fn is_verus(&self) -> bool {
        self.cargo_subcommand() == Some("verus")
    }

    pub(super) fn is_exact_cargo(&self, expected: &[&str]) -> bool {
        self.executable == Some(0)
            && self.executable_is("cargo")
            && self.cargo_full_arguments().iter().map(String::as_str).eq(expected.iter().copied())
    }

    pub(super) fn has_leading_assignments(&self) -> bool {
        self.executable.is_some_and(|index| index > 0)
    }

    fn is_exact_assigned_cargo(&self, assignment: &str, expected: &[&str]) -> bool {
        self.executable == Some(1)
            && self.words.first().is_some_and(|word| word == assignment)
            && self.executable_is("cargo")
            && self.cargo_full_arguments().iter().map(String::as_str).eq(expected.iter().copied())
    }

    pub(super) fn is_exact_command(&self, expected: &[&str]) -> bool {
        self.executable.is_some()
            && self.words.iter().map(String::as_str).eq(expected.iter().copied())
    }

    pub(super) fn is_exact_words(&self, expected: &[&str]) -> bool {
        self.words.iter().map(String::as_str).eq(expected.iter().copied())
    }

    pub(super) fn executable_is(&self, expected: &str) -> bool {
        self.executable
            .and_then(|index| self.words.get(index))
            .and_then(|word| Path::new(word).file_name())
            .is_some_and(|executable| executable == expected)
    }

    pub(super) fn has_argument(&self, expected: &str) -> bool {
        self.arguments().iter().any(|argument| argument == expected)
    }

    pub(super) const fn pipes_to_next(&self) -> bool {
        matches!(self.connector, Connector::Pipe)
    }

    pub(super) fn render(&self) -> String {
        self.words.join(" ")
    }

    fn cargo_subcommand(&self) -> Option<&str> {
        let mut skip_value = false;
        for argument in self.cargo_arguments() {
            if skip_value {
                skip_value = false;
                continue;
            }
            if argument.starts_with('+') {
                continue;
            }
            if option_takes_value(argument) {
                skip_value = true;
                continue;
            }
            if argument.starts_with('-') {
                continue;
            }
            return Some(argument);
        }
        None
    }

    fn cargo_arguments(&self) -> &[String] {
        let arguments = self.cargo_full_arguments();
        let boundary =
            arguments.iter().position(|argument| argument == "--").unwrap_or(arguments.len());
        &arguments[..boundary]
    }

    fn cargo_full_arguments(&self) -> &[String] {
        let Some(executable) = self.executable.filter(|_| self.executable_is("cargo")) else {
            return &[];
        };
        &self.words[executable + 1..]
    }

    fn arguments(&self) -> &[String] {
        let Some(executable) = self.executable else { return &[] };
        &self.words[executable + 1..]
    }
}

#[derive(Debug)]
pub(super) struct ParsedScript {
    commands: Vec<Command>,
    issues: BTreeSet<ScriptIssue>,
}

impl ParsedScript {
    pub(super) fn commands(&self) -> &[Command] {
        &self.commands
    }

    pub(super) fn is_failure_propagating(&self) -> bool {
        self.issues.is_empty()
            && self.commands.len() == 1
            && matches!(self.commands[0].connector, Connector::End | Connector::Sequence)
    }

    pub(super) fn has_no_shell_issues(&self) -> bool {
        self.issues.is_empty()
    }

    pub(super) fn is_reviewed_archive_install(&self) -> bool {
        let commands = &self.commands;
        self.issues.is_empty()
            && commands.len() == 9
            && commands[0].is_exact_command(&["set", "-euo", "pipefail"])
            && commands[1].is_exact_words(&["archive=$RUNNER_TEMP/verus.zip"])
            && commands[2].is_exact_words(&["install_root=$RUNNER_TEMP/peritus-verus"])
            && commands[3].is_exact_command(&[
                "curl",
                "--fail",
                "--location",
                "--retry",
                "3",
                "--output",
                "$archive",
                "https://github.com/verus-lang/verus/releases/download/release/$VERUS_VERSION/verus-$VERUS_VERSION-x86-linux.zip",
            ])
            && commands[4].pipes_to_next()
            && commands[4].is_exact_command(&[
                "printf",
                "%s  %s\\n",
                "$VERUS_LINUX_SHA256",
                "$archive",
            ])
            && commands[5].is_exact_command(&["sha256sum", "--check", "--strict"])
            && commands[6].is_exact_command(&["mkdir", "-p", "$install_root"])
            && commands[7]
                .is_exact_command(&["unzip", "-q", "$archive", "-d", "$install_root"])
            && commands[8].is_exact_command(&[
                "printf",
                "%s\\n",
                "$install_root/verus-x86-linux",
                ">>",
                "$GITHUB_PATH",
            ])
    }

    pub(super) fn exact_cargo_command(&self, expected: &[&str]) -> bool {
        self.is_failure_propagating() && self.commands[0].is_exact_cargo(expected)
    }

    pub(super) fn exact_docs_command(&self) -> bool {
        self.is_failure_propagating()
            && self.commands[0].is_exact_assigned_cargo(
                "RUSTDOCFLAGS=-D warnings",
                &["doc", "--workspace", "--all-features", "--no-deps", "--locked"],
            )
    }

    pub(super) fn is_reviewed_config_preflight(&self) -> bool {
        self.is_reviewed_root_config_preflight() || self.is_reviewed_candidate_config_preflight()
    }

    pub(super) fn is_reviewed_root_config_preflight(&self) -> bool {
        self.issues.is_empty()
            && self.commands.len() == 1
            && self.commands[0].is_exact_command(&[
                "git",
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--exit-code",
                PRE_CARGO_AUTHORITY,
                "--",
                ".cargo/config.toml",
                ".gitattributes",
            ])
    }

    pub(super) fn is_reviewed_candidate_config_preflight(&self) -> bool {
        self.issues.is_empty()
            && self.commands.len() == 1
            && self.commands[0].is_exact_command(&[
                "git",
                "-C",
                "candidate",
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--exit-code",
                PRE_CARGO_AUTHORITY,
                "--",
                ".cargo/config.toml",
                ".gitattributes",
            ])
    }
}

pub(super) fn parse_script(script: &str) -> ParsedScript {
    let mut parser = Parser::default();
    parser.parse(script);
    parser.finish()
}

#[derive(Default)]
struct Parser {
    commands: Vec<Command>,
    issues: BTreeSet<ScriptIssue>,
    words: Vec<String>,
    word: String,
    quote: Option<char>,
}

impl Parser {
    fn parse(&mut self, script: &str) {
        let mut characters = script.chars().peekable();
        while let Some(character) = characters.next() {
            if let Some(delimiter) = self.quote {
                if character == delimiter {
                    self.quote = None;
                } else {
                    if delimiter == '"'
                        && (character == '`' || character == '$' && characters.peek() == Some(&'('))
                    {
                        self.issues.insert(ScriptIssue::DynamicShell);
                    }
                    self.word.push(character);
                }
                continue;
            }
            match character {
                '\'' | '"' => self.quote = Some(character),
                '`' => {
                    self.issues.insert(ScriptIssue::DynamicShell);
                    self.word.push(character);
                }
                '$' | '<' | '>' if characters.peek() == Some(&'(') => {
                    self.issues.insert(ScriptIssue::DynamicShell);
                    self.word.push(character);
                }
                '\\' if characters.peek() == Some(&'\n') => {
                    characters.next();
                }
                '\\' => {
                    if let Some(escaped) = characters.next() {
                        self.word.push(escaped);
                    }
                }
                ' ' | '\t' | '\r' => self.push_word(),
                '|' if characters.peek() == Some(&'|') => {
                    characters.next();
                    self.issues.insert(ScriptIssue::FailureMasking);
                    self.finish_command(Connector::Or);
                }
                '|' => self.finish_command(Connector::Pipe),
                '&' if characters.peek() == Some(&'&') => {
                    characters.next();
                    self.finish_command(Connector::And);
                }
                '&' => {
                    self.issues.insert(ScriptIssue::BackgroundExecution);
                    self.finish_command(Connector::Background);
                }
                '\n' | ';' => self.finish_command(Connector::Sequence),
                '#' if self.word.is_empty() => {
                    characters.by_ref().find(|next| *next == '\n');
                    self.finish_command(Connector::Sequence);
                }
                _ => self.word.push(character),
            }
        }
    }

    fn finish(mut self) -> ParsedScript {
        self.finish_command(Connector::End);
        ParsedScript { commands: self.commands, issues: self.issues }
    }

    fn finish_command(&mut self, connector: Connector) {
        self.push_word();
        if self.words.is_empty() {
            return;
        }
        let words = std::mem::take(&mut self.words);
        let executable = words.iter().position(|word| !is_assignment(word));
        if let Some(name) = executable
            .and_then(|index| words.get(index))
            .and_then(|word| Path::new(word).file_name())
            .and_then(|word| word.to_str())
        {
            if is_opaque(name) || name.starts_with('$') {
                self.issues.insert(ScriptIssue::NestedShell);
            }
            if name == "!" {
                self.issues.insert(ScriptIssue::FailureMasking);
            }
            if is_control_word(name) {
                self.issues.insert(ScriptIssue::ShellControlFlow);
            }
        }
        self.commands.push(Command { words, executable, connector });
    }

    fn push_word(&mut self) {
        if !self.word.is_empty() {
            self.words.push(std::mem::take(&mut self.word));
        }
    }
}
