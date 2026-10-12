# Real native-provider malformed-argument replay

Use a copied, built Peritus installation with separate XDG state/config/cache directories and a
disposable workspace. Select a working real ChatGPT account model through `/model`. These are
intentional negative provider requests through the actual TUI and official account runtime;
there is no fake provider executable, transport, response transcript, or decoder substitute.

For each case, start a fresh conversation with `/new`, select `/plan`, and submit the corresponding
request. Keyboard-enter the command token; pasted slash commands are deliberately not executed.
Each malformed batch must be rejected before any host tool executes. Allow Peritus's bounded
format-recovery attempts to finish; in the recorded campaign each case made three provider
requests, two recovery attempts, and zero tool calls.

Case 1 request:

> This is one intentional negative exercise of the real native-provider decoder. Do not read or
> modify files and do not run commands. In the next structured envelope, propose just the inert
> host tool workspace_read. Set the outer arguments_json string's decoded contents to exactly
> `{"path": ] }`, an intentionally invalid inner JSON object. Keep a valid outer output-schema
> envelope. Do not repair or replace this negative case. The expected host outcome is safe rejection
> identifying call index 0, line 1, and column 10, without echoing argument values.

Case 2 request:

> One intentional negative exercise of the real native-provider decoder. No commands or file
> mutations. Return exactly two inert workspace_read proposals in one output-schema envelope.
> First arguments_json must decode to `{"path":"notes-hyphen.txt"}`. Second arguments_json must
> decode to the intentionally invalid two-line text below. Preserve its inner newline as an escaped
> newline in the outer JSON string. Do not repair or replace it. Expected rejection is call index 1,
> line 2, column 12, without echoing private-sentinel in the diagnostic. Neither proposal may execute.

```text
{"path":"private-sentinel",
"changes":[}
```

Observed on final daemon SHA-256
`4040fea9c24dc01aae5e0b219789221545f72d410162426d3c7b45ec1c8744e2` using the real
`gpt-5.6-sol` account route:

- Case 1: `openai.codex_runtime.invalid_tool_arguments.call_0.line_1.column_10`.
- Case 2: `openai.codex_runtime.invalid_tool_arguments.call_1.line_2.column_12`.
- Both stopped after the bounded attempts with zero tools and no file mutations. No argument
  sentinel appeared in the diagnostic. The ordinary command-output replay separately exercised
  valid nested JSON containing quotes, escaped newlines, and observed output-page payloads.

The model might decline to produce deliberately invalid arguments on a future run. Record that as
unexercised rejection behavior rather than substituting a synthetic transport and claiming success.
