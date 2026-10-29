# Automation setup bootstrap — navigation

## Responsibility

[bootstrap.rs](bootstrap.rs) downloads the pinned native uv archive for Linux, macOS, or
Windows on x86-64/ARM64, verifies its embedded SHA-256, and publishes only the private
bootstrap executable. Downloads never execute before checksum validation.

[linux.rs](linux.rs) repairs AppArmor user-namespace denial for exact managed Chromium
executables. It installs and loads a root-owned, cache-specific profile after ordinary
dependency installation is authorized, preserving Chromium's sandbox and global policy.

[Parent setup owner](../setup.rs) owns Python/package/browser provisioning, installation
serialization and readiness. [Parent waypoint](../AGENTS.md).
## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.
