# Peritus H2 packaging assets

These assets package the four target-native executables required for application and sandbox
qualification: `peritusd`, `peritus`, `peritus-tui`, and the platform's C3 sandbox helper. A
release builder writes the canonical `manifest.toml` and `SHA256SUMS` from
`peritus-platform-qualification::PackageManifest`, then places the files in the paths declared by
the target `package.toml.in`.

Installation is deliberately per-user because G0 authenticates local peers as the protected state
root owner. The operator must provision the strict `peritus.toml` and its public approval-registry
payload before enabling autostart. The installers never synthesize identities, authority,
credential references, project registrations, or tool/provider policy.

The native supervisors execute exactly:

```text
peritusd serve --config <absolute-platform-config-path>
```

There is no daemonization flag, remote listener, implicit endpoint flag, or invented service mode.
While live, `state/daemon.instance` reports the stable endpoint name. Unix clients use
`<state-root>/<endpoint>.sock`; Windows clients use `\\.\pipe\<endpoint>`.

Upgrade snapshots only package-owned files and rolls them back if authenticated readiness fails.
Ordinary uninstall removes binaries, helper, and supervisor registration while preserving the
operator configuration, durable state, and diagnostic/telemetry roots. Destructive purge of those
protected roots is outside these assets.
