# peritus-tools-computer

## Folder guide

Persistent browser and desktop automation host adapters for Peritus.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-tools-computer`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Browser/desktop control host adapters, persistent session RPC, installation/setup, screenshots, observations, and controller driver sources. |

Ordinary host adapters for real browser and native application automation. The bundled Python
driver keeps Playwright pages and app identities alive between requests. Peritus starts the
driver through C4/C2, and private authenticated loopback RPC carries bounded operations.
Screenshots are actual PNG artifacts, not generated images or textual base64 observations.
The attack-graph adapter also uses a fresh restricted Chromium context to correlate one pinned
stored-content signal with exact private HTTP and copied-source receipts; it cannot browse outside
the host-selected disposable laboratory origin.

Browser sessions use fresh profiles or explicitly selected CDP tabs. An optional private unpacked
Chrome extension can open or attach to loopback development pages and read explicitly shared
HTTPS tabs in the user's ordinary Chrome;
its native bridge, permissions, receipts, observation tokens, and owned-tab cleanup remain
host-controlled. External HTTPS reads require selection in its popup and end on origin changes,
tab closure or revocation. External interaction, files, credential export, arbitrary JavaScript
and store publication remain outside this bridge. Native Linux sessions use
AT-SPI actions and editable text, with X11 input and screenshots for compatible windows. Owned
test applications can use isolated Xvfb displays. Existing applications are detached during
cleanup. Unsupported interfaces produce explicit errors; X11 input is not advertised as control
of arbitrary Wayland windows.

Native installers and portable first launches automatically provision the browser runtime in
Peritus's user data directory. `peritus automation setup` repairs Python, Playwright, Pillow and
Chromium; `peritus automation status` reports dependency availability. No project test framework
is installed or invoked.

Install the private ordinary-Chrome bridge with
`peritus automation chrome-extension install`, then load the printed directory once through
Chrome's **Load unpacked** control. `peritus automation chrome-extension status` reports both
registration and live connection state. `uninstall` removes only Peritus's local assets and
native-host registration; Chrome still requires removing the unpacked extension entry itself.
