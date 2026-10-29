# compat/app-protocol/v1/minimal-workbench-file-import-upload — navigation

## Responsibility

`minimal-workbench-file-import-upload` is a `minimal` input case for `app-protocol` `v1`. The manifest identifies input bytes; expectation data states the intended decoder outcome.

## Files in this folder

| File | Role |
| --- | --- |
| [expectation.toml](expectation.toml) | Expected decoder outcome: accepted=`true`. |
| [fixture.toml](fixture.toml) | Compatibility manifest: `app-protocol` / `v1` / `minimal-workbench-file-import-upload`, kind `minimal`; lists input digests. |
| [payload.hex](payload.hex) | Retained canonical input bytes (hexadecimal text when the suffix is .hex). |

## Working in this area

Read [`fixture.toml`](fixture.toml) first to identify the protocol, version, case kind, and bound payloads, then evaluate its payload and expectation as one compatibility case. Decoder behavior lives in the owning Rust package, so change fixture bytes, recorded digests, and expected disposition together and use the parent waypoint to find the active contract.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
