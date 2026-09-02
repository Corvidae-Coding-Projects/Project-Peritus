# Peritus test support

`peritus-test-support` owns deterministic, protocol-neutral fixtures used by later Peritus
crates. It is a class-C testing crate and must be consumed only as a development dependency by
production crates.

The public surface provides an explicitly advanced clock, replayable counter identifiers that are
unique within one non-wrapping source, per-aggregate event fixture contexts, occurrence-addressed
fault injection, exact scripted calls and streams, provider/tool-branded scripts, canonical
compatibility fixture verification, a deterministic loopback HTTP server, and hardened
caller-rooted temporary Git repositories.
Identifier sources with the same namespace intentionally replay the same bytes; independent
sources are disjoint only when callers select distinct namespaces. Clone semantics are deliberate:
clock and fault-injector clones share state; identifier sources, event builders, scripts, streams,
and repositories do not clone.

This crate does not define production clock, identifier, provider, tool, event, protocol, or
storage traits. Script outcomes are caller-owned types, and event fixture contexts contain only
the A1 `EventId` and `EventSequence` primitives. Later protocol owners adapt their own types to
these mechanics in tests without creating an outward production dependency.

`FakeHttpServer` binds a fresh `127.0.0.1` listener for one case and owns its worker through join.
It matches a bounded request exactly, then writes caller-scripted status, headers, and body chunks.
Scripts can close after the response head or an exact chunk count and can pause at a selected
release point for cancellation tests. Dropping the server wakes a blocked accept, closes an active
socket, releases a paused worker, and joins it; no worker is detached. `finish` returns the exact
match result, response progress, and termination classification.

`FakeHttpSequenceServer` uses the same redacted exchange model for a bounded nonempty sequence on
one stable endpoint. It exists for production retry-path tests: every attempt has its own exact
request/response script, all attempts are returned in order, and drop or `finish` still joins the
single owned worker. It performs no implicit retry and cannot accept more exchanges than the
configured sequence.

Captured requests are redacted by construction. They retain only a validated method, byte counts,
SHA-256 digests, normalized header names, sensitive-name classification, and the direct match
result. Request targets, header values, and bodies are not retained, and expectation/script debug
output reports their sizes rather than their bytes. Wire fixtures remain generic HTTP; provider-
specific JSON and event-stream payloads belong in adapter tests.

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

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-test-support
```
