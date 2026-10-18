# Automation setup bootstrap — navigation

[bootstrap.rs](bootstrap.rs) downloads the pinned native uv archive for Linux, macOS, or
Windows on x86-64/ARM64, verifies its embedded SHA-256, and publishes only the private
bootstrap executable. Downloads never execute before checksum validation.

[Parent setup owner](../setup.rs) owns Python/package/browser provisioning, installation
serialization and readiness. [Parent waypoint](../AGENTS.md).
