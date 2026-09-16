# Context-capacity mutation reachability and canary

- Source: `c42327c1fcd27ba2e8f8b27da541d94923b7c89f`
- Host: Linux x86_64
- Tool: cargo-mutants 27.1.0

The automatic `peritus-context/src/working/selection.rs` inventory produced `[]`. The maintained
`discovery-mutation-context` command therefore wrote status `unreachable`, kept its baseline as
`not_run`, and exited nonzero. It did not report a passing zero-mutation campaign. The selection
body is inside a Verus macro, which this cargo-mutants version does not instrument.

The maintained canary copied the committed source to an isolated disposable clone and removed the
hard check that rejects a required closure larger than the available token capacity:

```text
needed <= allocation || needed > available_tokens
```

became `needed <= allocation`. The focused
`headroom_preserves_required_roots_and_shared_dependencies_without_optional_expansion` test failed
at its `tokens - 1` capacity assertion. This is behavioral detection by the designated test, not a
compiler or proof rejection. The command recorded `baseline=passed`, `mutant=detected`, and mutant
exit code 101. The clone was removed and the mutant was never committed.
