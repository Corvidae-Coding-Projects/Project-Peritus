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
| [src/](src/AGENTS.md) | Optional browser/desktop control host adapters, persistent session RPC, installation/setup, screenshots, observations, and controller driver sources. |

Ordinary host adapters for real browser and native application automation. The bundled Python
driver keeps Playwright pages and app identities alive between requests. Peritus starts the
driver through C4/C2, and private authenticated loopback RPC carries bounded operations.
Screenshots are actual PNG artifacts, not generated images or textual base64 observations.

Browser sessions use fresh profiles or explicitly selected CDP tabs. Native Linux sessions use
AT-SPI actions and editable text, with X11 input and screenshots for compatible windows. Owned
test applications can use isolated Xvfb displays. Existing applications are detached during
cleanup. Unsupported interfaces produce explicit errors; X11 input is not advertised as control
of arbitrary Wayland windows.

`peritus automation setup` installs the optional Python browser runtime into Peritus's user data
directory. `peritus automation status` reports dependency availability. No project test framework
is installed or invoked.
