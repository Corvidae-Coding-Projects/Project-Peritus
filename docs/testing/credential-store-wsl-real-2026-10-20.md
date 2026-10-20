# WSL credential-store guidance — 2026-10-20

## Outcome and scope

On WSL2 Debian, provider setup reported
`PERITUS-SECRETS-006 during Probe: platform credential store is unavailable` after the operator had
pasted a hidden API key and after model discovery had already listed the provider's advertised
models. The same key worked in native Windows 11. The ordering made the failure look like a
provider or model problem when the store had simply never been reachable.

On Linux and WSL the platform store is the freedesktop Secret Service reached over the session
D-Bus. A default WSL distribution runs neither, so the one-time store initialization behind the
probe fails and no key can be written. Windows uses Credential Manager, which is why the same key
was fine there. Nothing about the provider, the key, or the OpenCode runtime was wrong.

Provider setup now probes the store before it asks for a secret. When the store is unavailable it
stops with corrective guidance instead of collecting a hidden key and running discovery first; on
Linux and WSL the guidance names the Secret Service and points at
[c3-platform-security.md](../c3-platform-security.md#linux-and-wsl-credential-store), which records
the exact commands. Account providers (ChatGPT and Claude sign-in) never used this store and are
unchanged, as is the existing-route path that tests or replaces an already configured key.

## Confirmed defects

| Defect | Before | After / evidence |
| --- | --- | --- |
| An unreachable credential store is reported only after hidden key entry and model discovery. | Selecting OpenCode Go on WSL printed the credential prompt, the advertised model list, and then `PERITUS-SECRETS-006 during Probe: platform credential store is unavailable`, with no corrective step. | The store is probed first. Both an API provider and OpenCode Go print the guidance and exit 70 without ever asking for a key. |
| No operator guidance exists for the WSL Secret Service. | The operator had to guess which password a keyring unlock prompt wanted. | Setup names the store, states that a session D-Bus and an unlocked gnome-keyring are required, and points at the recorded commands. |

## Actual operation and results

Every row is real pty output from `target/debug/peritus providers` with isolated
`XDG_CONFIG_HOME`, `XDG_STATE_HOME`, and `XDG_CACHE_HOME` roots. The placeholder credential is the
literal `sk-dummy-not-a-real-key`; no real key is read and no inference is requested.

| Interface / sequence | Inputs and independent expectation | Observation |
| --- | --- | --- |
| OpenAI API, no session bus | Guidance is printed, no key is requested, setup exits 70 | Guidance printed directly after the provider label; exit 70; the key prompt never appeared. |
| OpenCode Go, no session bus | Same, reached after the OpenCode runtime check | The runtime check passed against the official Linux 1.19.0; guidance printed; exit 70. |
| OpenCode Go, session bus and unlocked keyring | The key prompt appears, the credential is stored, settings are saved | No guidance; key accepted; the vendor CLI advertised 31 models; the credential was written; `Provider settings saved.`; exit 0. |
| Reopen settings, then deselect | The stored route is retained and deselected | The catalog showed `OpenCode Go via OpenCode CLI  selected, configured; connection not tested this session`; after deselecting, settings saved with no "could not be removed automatically" warning. |
| Reopen settings again | The route is gone | The catalog shows `Add key` for OpenCode Go. |
| Independent Secret Service inventory | The Peritus provider namespace holds no credential | `SearchItems` for `service=org.corvidae-coding.peritus.providers` returned `aoao 0 0` after the cleanup run. |

The retained driver replayed the whole sequence as 21 checks with 0 failures, including the two
unavailable cases, the stored and removed credential, and the independent store inventory.

## Retained evidence

- [Reproducer](reproducers/credential-store-unavailable/README.md) — pty driver, five scenarios, and
  the Secret Service inventory helper.
- [Observed baseline](reproducers/credential-store-unavailable/observed-baseline.txt) — verbatim
  transcripts and the recorded check outcomes.

## Limits

- The campaign needs a shell without the repository sandbox: it starts a session bus and runs the
  OpenCode version probe, and the sandbox denies socket creation and interop.
- The probe reports reachability, not readiness. A store that is reachable but locked, or reachable
  without a usable login collection, still passes the probe and fails on the write with its own
  outcome class; the guidance therefore tells the operator to unlock the keyring before setup.
- `available-opencode-go` shows that the store accepted the write and that the flow still saves
  provider settings. It is not provider connectivity, model quality, or inference evidence.
- The macOS and Windows guidance branches are single-line unlock/repair messages and were not
  exercised on this host.
