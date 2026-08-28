#[derive(Clone, Copy)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
    Pwsh,
}

impl Shell {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "bash" => Some(Self::Bash),
            "zsh" => Some(Self::Zsh),
            "fish" => Some(Self::Fish),
            "powershell" => Some(Self::Pwsh),
            _ => None,
        }
    }
}

pub fn generate(shell: Shell) -> String {
    match shell {
        Shell::Bash => BASH.to_owned(),
        Shell::Zsh => ZSH.to_owned(),
        Shell::Fish => FISH.to_owned(),
        Shell::Pwsh => POWERSHELL.to_owned(),
    }
}

const BASH: &str = r#"_peritus() {
  local cur prev
  COMPREPLY=()
  cur="${COMP_WORDS[COMP_CWORD]}"
  prev="${COMP_WORDS[COMP_CWORD-1]}"
  case "$prev" in
    peritus) COMPREPLY=( $(compgen -W "providers status shutdown command events artifact prompt terminal completions help" -- "$cur") ) ;;
    command) COMPREPLY=( $(compgen -W "submit" -- "$cur") ) ;;
    events) COMPREPLY=( $(compgen -W "watch" -- "$cur") ) ;;
    artifact) COMPREPLY=( $(compgen -W "get put cancel" -- "$cur") ) ;;
    prompt) COMPREPLY=( $(compgen -W "answer cancel" -- "$cur") ) ;;
    terminal) COMPREPLY=( $(compgen -W "attach input resize detach cancel" -- "$cur") ) ;;
    completions) COMPREPLY=( $(compgen -W "bash zsh fish powershell" -- "$cur") ) ;;
    *) COMPREPLY=( $(compgen -W "--endpoint --session --timeout-seconds --json --help --version --wait --actor --envelope --payload --idempotency-key --no-expected-revision --topic --after --window --count --snapshot-acceptable --artifact --transfer --output --force --input --media-type --chunk-size --binding --signed-decision --text --selection --confirm --secret-reference --rationale --attachment --process --originating-request --columns --rows --no-follow" -- "$cur") ) ;;
  esac
}
complete -F _peritus peritus
"#;

const ZSH: &str = r#"#compdef peritus
_peritus() {
  local -a commands
  commands=(providers status shutdown command events artifact prompt terminal completions help)
  _arguments '*::arg:->args'
  if (( CURRENT == 2 )); then _describe 'command' commands; fi
}
_peritus "$@"
"#;

const FISH: &str = r"complete -c peritus -f
complete -c peritus -n '__fish_use_subcommand' -a 'providers status shutdown command events artifact prompt terminal completions help'
complete -c peritus -l endpoint -r
complete -c peritus -l session -r
complete -c peritus -l timeout-seconds -r
complete -c peritus -l json
complete -c peritus -l help -s h
complete -c peritus -l version -s V
";

const POWERSHELL: &str = r#"Register-ArgumentCompleter -Native -CommandName peritus -ScriptBlock {
  param($wordToComplete, $commandAst, $cursorPosition)
  'providers','status','shutdown','command','events','artifact','prompt','terminal','completions','help' |
    Where-Object { $_ -like "$wordToComplete*" } |
    ForEach-Object { [System.Management.Automation.CompletionResult]::new($_, $_, 'ParameterValue', $_) }
}
"#;
