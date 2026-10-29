# ChatGPT direct OAuth navigation

Peritus owns this optional registration and credential lifecycle independently of Codex. Tokens
remain in bounded, protected records; callers receive account metadata and opaque references.

| File | Responsibility |
| --- | --- |
| [mod.rs](mod.rs) | Public account metadata and internal validated, zeroizing session records. |
| [login.rs](login.rs) | Browser authorization, PKCE/state/nonce, loopback callback, verified identity publication. |
| [network.rs](network.rs) | Fixed-origin OAuth exchange/refresh, JWKS ID-token verification, catalogs and revocation. |
| [store.rs](store.rs) | Private atomic files, host identity and cross-process session-operation locking. |
| [provider.rs](provider.rs) | Renew credentials, discover current models and compose the direct Responses provider. |

The request profile is [request/chatgpt.rs](../request/chatgpt.rs). Launcher provider settings
own explicit account selection. The daemon factory composes the adapter; host tools retain their
normal authority checks. The existing [Codex runtime](../runtime/AGENTS.md) is a separate route.
