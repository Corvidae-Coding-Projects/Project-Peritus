use super::workflow_commands::ParsedScript;

pub(super) fn is_reviewed_install(script: &ParsedScript) -> bool {
    let commands = script.commands();
    script.has_no_shell_issues()
        && commands.len() == 9
        && commands[0].is_exact_command(&["set", "-euo", "pipefail"])
        && commands[1].is_exact_words(&["archive=$RUNNER_TEMP/actionlint.tar.gz"])
        && commands[2].is_exact_words(&["install_root=$RUNNER_TEMP/peritus-actionlint"])
        && commands[3].is_exact_command(&[
            "curl",
            "--fail",
            "--location",
            "--retry",
            "3",
            "--output",
            "$archive",
            "https://github.com/rhysd/actionlint/releases/download/v$ACTIONLINT_VERSION/actionlint_${ACTIONLINT_VERSION}_linux_amd64.tar.gz",
        ])
        && commands[4].pipes_to_next()
        && commands[4].is_exact_command(&[
            "printf",
            "%s  %s\\n",
            "$ACTIONLINT_LINUX_SHA256",
            "$archive",
        ])
        && commands[5].is_exact_command(&["sha256sum", "--check", "--strict"])
        && commands[6].is_exact_command(&["mkdir", "-p", "$install_root"])
        && commands[7].is_exact_command(&[
            "tar",
            "-xzf",
            "$archive",
            "-C",
            "$install_root",
            "actionlint",
        ])
        && commands[8].is_exact_command(&[
            "printf",
            "%s\\n",
            "$install_root",
            ">>",
            "$GITHUB_PATH",
        ])
}
