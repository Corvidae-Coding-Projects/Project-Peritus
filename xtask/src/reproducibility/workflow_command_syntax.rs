const NON_RESOLVING_SUBCOMMANDS: [&str; 14] = [
    "clean",
    "fmt",
    "help",
    "init",
    "locate-project",
    "login",
    "logout",
    "new",
    "owner",
    "report",
    "search",
    "version",
    "yank",
    "--version",
];
const OPTIONS_WITH_VALUES: [&str; 7] =
    ["--color", "--config", "--lockfile-path", "--manifest-path", "--target", "--target-dir", "-Z"];
const OPAQUE_EXECUTABLES: [&str; 21] = [
    ".",
    "bash",
    "command",
    "dash",
    "env",
    "eval",
    "exec",
    "nice",
    "node",
    "perl",
    "powershell",
    "pwsh",
    "python",
    "python3",
    "ruby",
    "sh",
    "source",
    "sudo",
    "timeout",
    "xargs",
    "zsh",
];
const CONTROL_WORDS: [&str; 10] =
    ["case", "do", "done", "else", "esac", "fi", "for", "if", "until", "while"];

pub(super) fn is_non_resolving(subcommand: &str) -> bool {
    NON_RESOLVING_SUBCOMMANDS.contains(&subcommand)
}

pub(super) fn option_takes_value(argument: &str) -> bool {
    OPTIONS_WITH_VALUES.contains(&argument)
}

pub(super) fn is_opaque(executable: &str) -> bool {
    OPAQUE_EXECUTABLES.contains(&executable)
}

pub(super) fn is_control_word(word: &str) -> bool {
    CONTROL_WORDS.contains(&word)
}

pub(super) fn is_assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| {
        !name.is_empty()
            && name.bytes().enumerate().all(|(index, byte)| {
                byte == b'_' || byte.is_ascii_alphabetic() || index > 0 && byte.is_ascii_digit()
            })
    })
}
