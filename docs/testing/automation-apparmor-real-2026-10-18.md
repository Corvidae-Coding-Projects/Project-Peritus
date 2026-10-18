# Ubuntu managed-browser sandbox repair

## Confirmed release failure

Native Linux x86-64 and ARM64 packages compiled at `8dbb9273aff9ecde9743ec5fd37185121ac50220`.
Their actual installer then failed while `peritus automation setup` launched downloaded
Chromium with sandboxing enabled. Retained installation logs contain `No usable sandbox!`
and `Chromium sandboxing failed!`. Ubuntu 24.04 restricts unprivileged user namespaces;
the downloaded browser paths did not have an AppArmor allowance.

Original release run: https://github.com/Corvidae-Coding-Projects/Project-Peritus/actions/runs/39095381162
The `startup-ubuntu-24.04` and `startup-ubuntu-24.04-arm` artifacts retain the original
installer output. This is an actual fresh-install product defect, distinct from the earlier
Mac replay fixture incorrectly setting `PERITUS_INSTALL_DEPS=0`.

## Repair and prospective acceptance

Managed setup retries missing-library and AppArmor repairs independently, once each.
The pinned Playwright registry supplies the actual full Chromium and headless-shell paths
and an independent cache root. Canonical targets must be regular executable files owned by
the current user within that root. Setup parses the generated policy before requesting
administrator authorization to install/load it. Literal path syntax is escaped; stable
UID/cache profile names replace old executable attachments on upgrades. Cached readiness is
rechecked on AppArmor-restricted hosts, so unloading a policy can be repaired on launch.

Global sysctls and `chromium_sandbox=True` remain unchanged. The allowance is path-based:
the same user can replace an executable at its authorized path. Explicit runtime ownership
and the dependency-installation opt-out remain applicable.

The [retained replay](reproducers/automation-apparmor.py) builds the actual public Rust
setup consumer and operates real Playwright browsers on disposable Ubuntu x86-64/ARM64
runners. Expectations were fixed before execution:

- Cold setup with dependency installation disabled fails without publishing readiness.
- Ordinary setup succeeds under the existing AppArmor user-namespace restriction.
- A fresh process reopens readiness; unloading the managed profile is repaired by `ensure_setup`.
- Both managed headless shell and full Chromium navigate a real local HTTP page, execute
  a Unicode DOM mutation, and read it back with sandboxing enabled.
- CDP identifies this browser's actual renderer processes; their command lines have no
  sandbox-disabling flags, and their process status reports seccomp and no-new-privileges.
- An adjacent copied executable remains blocked, detecting unintended broad attachments.
- Cache paths containing spaces, Unicode, glob syntax, quotes, backslashes, and variable
  syntax work literally; `PLAYWRIGHT_BROWSERS_PATH=0` uses the actual package-local cache.
- Installed profiles are root-owned and not writable by the user; the global restriction
  is byte-for-byte unchanged.

Run the focused real replay with:

```sh
gh workflow run release.yml --ref develop --field runtime_replay=true
```

This mode runs only the public consumer and actual browsers; it does not request provider
inference or start native release builds. Final release acceptance separately requires the
actual tagged package installation and offline TUI startup on all four Unix targets.

## Results

Local Linux compilation passed with two build workers. Focused Ubuntu execution and final
release replay are pending; no passing execution is inferred from compilation or review.
No unit tests, mocks, formal verification, or repository verification suites were run.

The selective policy follows [Chromium's upstream guidance](https://chromium.googlesource.com/chromium/src/+/main/docs/security/apparmor-userns-restrictions.md).
Path escaping and unprivileged parser prevalidation were independently reviewed against
the [AppArmor 4 parser sources](https://gitlab.com/apparmor/apparmor/-/tree/apparmor-4.0/parser).
