# Peritus test support

`peritus-test-support` owns deterministic, protocol-neutral fixtures used by later Peritus
crates. It is a class-C testing crate and must be consumed only as a development dependency by
production crates.

The public surface provides an explicitly advanced clock, replayable counter identifiers that are
unique within one non-wrapping source, per-aggregate event fixture contexts, occurrence-addressed
fault injection, exact scripted calls and streams, provider/tool-branded scripts, canonical
compatibility fixture verification, and hardened caller-rooted temporary Git repositories.
Identifier sources with the same namespace intentionally replay the same bytes; independent
sources are disjoint only when callers select distinct namespaces. Clone semantics are deliberate:
clock and fault-injector clones share state; identifier sources, event builders, scripts, streams,
and repositories do not clone.

This crate does not define production clock, identifier, provider, tool, event, protocol, or
storage traits. Script outcomes are caller-owned types, and event fixture contexts contain only
the A1 `EventId` and `EventSequence` primitives. Later protocol owners adapt their own types to
these mechanics in tests without creating an outward production dependency.

Compatibility cases use this layout:

```text
compat/<surface>/<surface-version>/<case>/fixture.toml
```

The version-one manifest contains `schema`, `surface`, `surface_version`, `case`, `kind`, and a
strictly sorted `files` array whose entries contain a portable relative `path` and lowercase
SHA-256 digest. Manifested bytes are never normalized. Every nonempty surface/version group must
contain `minimal`, `realistic`, `corrupt`, and `adversarial` cases; an empty pre-release catalog
is allowed only through an explicit coverage policy and returns an observable `EmptyPreRelease`
result rather than covered status.
