# peritus-tool-router

`peritus-tool-router` is the control point between an agent's requested tool call and a concrete
tool implementation. It registers available tools, exposes only the capabilities permitted for the
current role, prepares calls, checks the exact authority granted by policy, and records enough
bounded state to replay or recover a dispatch safely.

The router sits above the tool protocol and below orchestration. It does not execute shell, Git, or
filesystem operations itself, and it cannot invent permissions. Execution stays in the dedicated
tool crates; authority stays in the policy and approval layers.

Registration, preparation, authorization, dispatch permits, and replay controls remain separate so
providers and tools can evolve without turning the router into a monolithic executor.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-tool-router
```
