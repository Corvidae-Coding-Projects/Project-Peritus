# compat/app-protocol/v1/adversarial-foreign-family — navigation

## Responsibility

`adversarial-foreign-family` is a `adversarial` input case for `app-protocol` `v1`. The manifest identifies input bytes; expectation data states the intended decoder outcome.

## Files in this folder

| File | Role |
| --- | --- |
| [expectation.toml](expectation.toml) | Expected decoder outcome: accepted=`false`, error `unsupported-family`. |
| [fixture.toml](fixture.toml) | Compatibility manifest: `app-protocol` / `v1` / `adversarial-foreign-family`, kind `adversarial`; lists input digests. |
| [payload.hex](payload.hex) | Retained canonical input bytes (hexadecimal text when the suffix is .hex). |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
