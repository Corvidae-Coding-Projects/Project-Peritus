# Credential-store availability replay

## Folder guide

Real-interface replay resources for provider setup when the operating-system credential store is
unreachable; start here and use [AGENTS.md](AGENTS.md) for file-level responsibilities.

### Local files

| File | Role |
| --- | --- |
| [README.md](README.md) | Human guide and operating/background notes for this folder. |
| [AGENTS.md](AGENTS.md) | Local navigation map and file responsibilities. |
| [drive.py](drive.py) | pty driver that answers scripted prompts and records exact terminal output. |
| [replay.py](replay.py) | Five provider-settings scenarios plus the Secret Service inventory check. |
| [secret-service-inventory.sh](secret-service-inventory.sh) | Lists (and with `--remove`, deletes) Peritus provider items in the session Secret Service. |
| [observed-baseline.txt](observed-baseline.txt) | Verbatim transcripts of the recorded run and its 21 check outcomes. |

## What this case shows

Provider setup probes the platform credential store before it asks for a secret. On Linux and WSL
that store is the freedesktop Secret Service, and a default WSL distribution has neither a session
D-Bus nor a keyring. The probe therefore fails, and setup stops with corrective guidance instead of
collecting a hidden key and running model discovery before reporting `PERITUS-SECRETS-006`.

The same flow must be unchanged when the store is reachable: the key prompt appears, the credential
is written to the store, and provider settings are saved.

## Prerequisites

- Linux or WSL2 with `dbus-daemon`, `gnome-keyring-daemon`, and `busctl` installed.
- A supported official OpenCode 1.19.0 for the OpenCode Go scenarios, pointed at with
  `--path-prefix DIR`; otherwise setup offers to install the runtime first.
- A sandbox-free shell: the campaign creates a session bus and the OpenCode version probe needs the
  same freedom. The replay keeps its own `XDG_CONFIG_HOME`, `XDG_STATE_HOME`, and `XDG_CACHE_HOME`
  under `--output`, so the operator's Peritus state is untouched.
- The operator's keyring must be unlocked, which is exactly the state the guide describes. An
  interrupted run can leave one placeholder credential behind; remove it with
  `secret-service-inventory.sh --remove`.

## Replay

```sh
cargo build --locked -p peritus-cli
python3 replay.py --binary target/debug/peritus \
    --output /tmp/peritus-credential-store --path-prefix /path/to/opencode-1.19.0
```

Each scenario writes its transcript to `<output>/transcripts`. The script exits non-zero if any
check fails.

## Expectations

| Scenario | Expected |
| --- | --- |
| `unavailable-openai-api` | Corrective guidance, no key prompt, exit 70. |
| `unavailable-opencode-go` | Same, reached through the OpenCode Go runtime check. |
| `available-opencode-go` | No guidance, key prompt, credential stored, settings saved, exit 0. |
| `cleanup-opencode-go` | Stored route retained, deselection removes the credential without a warning. |
| `after-cleanup-opencode-go` | The route is back to `Add key`. |
| `secret-service-inventory` | The Peritus provider namespace holds no credential. |

## Limits

- The credential is the literal placeholder `sk-dummy-not-a-real-key`; no real key is read, printed,
  or required. Model discovery lists vendor-advertised metadata and requests no inference.
- `available-opencode-go` shows that the store accepted the write and that the flow still saves
  provider settings. It is not a provider-connectivity or model-quality result.
- The unavailable scenarios exercise the Linux and WSL probe outcome only. macOS and Windows
  stores are not reachable from this host.
