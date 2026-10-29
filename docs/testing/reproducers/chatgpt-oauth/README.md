# Real direct OAuth and steering verification

These consumers use actual production libraries, official OpenAI endpoints, an interactive
OAuth grant, and real model responses. They never fabricate authentication or inference.
Use an absolute private session root. The login prints an authorization URL without tokens;
complete it in a browser on the host with the callback listener. Do not paste callback codes or
credential files into logs. `verify` makes up to three small requests using plan allowance.

Build the actual product, then link:

```sh
cargo build --locked --jobs 2 -p peritus-cli -p peritus-daemon
python3 docs/testing/reproducers/chatgpt-oauth/link.py --deps target/debug/deps --output /tmp/oauth-consumer
/tmp/oauth-consumer login /absolute/private/session-root
/tmp/oauth-consumer catalog /absolute/private/session-root
/tmp/oauth-consumer verify /absolute/private/session-root MODEL_FROM_CURRENT_CATALOG
```

The saved registration is independent of Codex. Login validates identity before saving, and
catalog/verify reload the actual protected registration. The same-session profile uses public
Responses, stateless replay, a namespaced host function and actual stream terminals. These
three connection checks prove generation, function selection and a tool-result continuation;
they do not prove workspace command execution, token refresh rotation, revocation or all models.

For the steering replay, link the existing negotiated IPC client against the same daemon
cohort using `incremental-context/link.py --control-output /tmp/control-client` (its required
`--output` can point to a separate temporary reuse consumer). Then run a fresh fixture:

```sh
python3 docs/testing/reproducers/chatgpt-oauth/steering.py \
  --daemon target/debug/peritusd --cli target/debug/peritus --client /tmp/control-client \
  --output /absolute/fresh/evidence --case question --question
```

The fixture admits a real long text request, submits a question while it is pending, and checks
replacement-request latency, the actual answer, settled host accounting, idle state and no tool
execution. The native provider is the official Codex route, using `gpt-6.1-sol` and its existing
login. It exercises the shared input boundary independently of direct OAuth authorization.
`--pause` instead checks that after-operation pause retains the pending operation during a
three-second observation. `--old` observes the previous binary for 12 seconds and expects no
replacement request; that is a bounded reproduction of the old delay, not an inference canary.
All daemon state and fixture content stay under the fresh evidence root. Cleanup targets only
that disposable daemon's endpoint. Existing user runs are never selected.

To exercise the same steering boundary through the new direct route, pass the saved non-secret
`peritus-chatgpt-oauth-v1:/absolute/path/to/registration.json` reference with `--oauth-reference`.
`--after-text` waits for actual streamed assistant text before submitting the question. This
covers the path after waiting notices stop, rather than only cancellation during initial latency.
The reference is a protected file location, never a token value.

`tools.py` uses a fresh disposable workspace and the same negotiated IPC client. `--case read-plan`
requires actual two-file plan results and their exact values in the public answer. `--case coding`
requires an actual patch, a native verification command and independent execution of the written
module; completion must reach Achieved with independent review and all requests settled.
For example, using the same protected registration reference as the steering replay:

```sh
python3 docs/testing/reproducers/chatgpt-oauth/tools.py \
  --daemon target/debug/peritusd --cli target/debug/peritus --client /tmp/control-client \
  --output /absolute/fresh/tool-evidence --case coding \
  --oauth-reference peritus-chatgpt-oauth-v1:/absolute/path/to/registration.json
```

The replay creates its own random task value and verifies the actual module against that value.
`--case command` is retained as a negative
scope probe: daemon goal handoff currently uses WorkspaceChanges, so a command-only task can
execute correctly yet fail empty-candidate qualification. Its native command receipt does not
prove candidate acceptance. No qualification gate is weakened for this probe.

The evidence ledger in `.design/openai-chatgpt-oauth.md` distinguishes passed real journeys,
preserved failures and unqualified flows. In particular, real cross-process refresh rotation was
verified at natural expiry, while sign-out and multiple-account behavior have source review only.
