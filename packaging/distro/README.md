# Native Debian and RPM releases

These are distribution builds, not archives renamed to `.deb` or `.rpm`.
The Debian 13 Dockerfile runs `dpkg-buildpackage` and debhelper; the Fedora 44
Dockerfile runs the checked-in [RPM spec](../rpm/peritus.spec) with `rpmbuild -ba`.
Both compile all four Linux product binaries from the same locked source tree,
with the `system-package` feature, inside a network-disabled container.
Supported native architectures are x86-64 and ARM64. The selected first release
version is `0.0.1`; CI signing refuses the `0.0.0` qualification placeholder.

## Build locally

Use a native x86-64 or ARM64 Linux build host. Prerequisites: the repository's
pinned Rust toolchain, Python 3.12 or newer, Docker, Git, and GnuPG. Podman is supported through
`PERITUS_CONTAINER_ENGINE=podman`. Run these from the repository root:

```sh
export PERITUS_PACKAGE_FORMAT=deb       # or rpm
export PERITUS_PACKAGE_MAINTAINER='Your name <your-email@example.org>'
cargo xtask distro-test
cargo xtask distro-image
cargo xtask distro-build
```

Image construction and `cargo vendor --locked` need network access. Package
compilation and tests use `--frozen`, the vendored dependencies, and no network.
The Dockerfiles pin multi-architecture base-image digests and the Rust version;
distribution build dependencies are resolved when the builder is created.
The exact builder image ID is recorded, not asserted to be timeless or bitwise
reproducible. CI saves and restores that same image between jobs.

Local builds default to two Cargo jobs. `PERITUS_PACKAGE_BUILD_JOBS` accepts
only `1`, `2`, `3`, or `4`; it controls Cargo compilation and Debian's package
build parallelism and is retained in the build observation. The public Linux
release jobs select four to use their existing
[four-CPU standard runners](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).
This does not alter the release profile, skip package tests, enable network access,
or extend the ten-minute CI job ceiling. Signing and verification keep their
existing two-job setting. Hosted completion still requires observed validation.

CI separates `cargo xtask distro-compile` from `cargo xtask distro-package-compiled`.
RPM adds `cargo xtask distro-compile-checks` between them to compile its test harness
in the native build environment without installing files or producing package payloads.
The final RPM recipe still executes those tests; precompilation is not test evidence.
The first command invokes only the native recipe's compilation stage and retains the complete
build tree in a same-run, format- and architecture-specific intermediate artifact.
Each subsequent phase verifies its archive hash and exact source inventory, commit, maintainer,
native architecture, builder-image identity, capacity, and workflow-run binding before
extracting into a fresh directory. The transfer preserves permissions and timestamps;
it is not a cross-run cache and is never attached as a public release asset.
The v2 intermediate record identifies the release or check-compilation stage.
RPM's checked record retains the preceding release-compilation record and requires a
distinct, later invocation with the same binding. Final RPM assembly rejects a
release-only tree. Both stages preserve the bounded, link-free archive contract.
Locally, the three RPM commands can run in order without moving or replacing an input:
the first writes `target/distro-compiled`, the second writes
`target/distro-check-compiled`, and assembly consumes the latter. Debian continues
to consume its release compilation directly.

Final packaging runs the ordinary full recipe with Debian's
[`--no-pre-clean`](https://manpages.debian.org/trixie/dpkg-dev/dpkg-buildpackage.1.en.html)
or RPM's [`--noprep`](https://rpm.org/docs/6.0.x/man/rpmbuild.1), since the admitted
tree has already been compiled or unpacked respectively. Cargo rechecks that tree;
all package tests, source and binary packaging, debug symbols, and default payload
compression remain enabled. No RPM `--short-circuit` or test-skipping option is used.
Debian's rules explicitly export `dpkg-buildflags` before either entry point, so
the compile stage receives the same native hardening flags as the full recipe.
The final build record retains the compilation's archive hash, source binding,
host, invocation, and real times alongside the final packaging observations.
Local `cargo xtask distro-build` still performs the complete build in one invocation.

Output is `dist/packages/deb` or `dist/packages/rpm`. Existing output is never
overwritten. Move a previous qualification set aside before rebuilding.
The build workspace is retained under `target/package-<format>-*`, including
on failure. Debian output includes the original vendored source tarball,
Debian packaging tarball, `.dsc`, `.buildinfo`, `.changes`, and binary packages.
RPM output includes the SRPM and binary/debug packages.

`peritus-<format>-build.json` records the Git commit, source-file SHA-256
inventory, version, architecture, source epoch, builder image ID, and unsigned
package hashes. Local uncommitted source is permitted for qualification and is
identified by its file hashes; CI checks that inventory against the tagged
checkout before signing. Vendored crates retain Cargo's checksum files and
upstream license notices. Missing reviewed upstream notices have exact-version
[provenance](../licenses/README.md); new unidentified omissions fail the build.

RPM package metadata uses the exact source epoch for build time and timestamp
clamping, and the declared stable build-host label `peritus-reproducible`, through
RPM's supported [reproducibility controls](https://rpm.org/docs/6.0.x/man/rpmbuild-config.5).
The build record separately retains the actual host, fresh invocation identity,
and observed start/finish times. That record and signatures are genuine external
observations, not normalized package content. Deterministic metadata alone does
not prove that independent complete package builds are byte-identical.

The recipes preserve debug information for the distribution's own debug-package
tools and use normal package-manager dependency discovery. The four binaries
are siblings under `/usr/lib/peritus`; `/usr/bin/peritus` is a symlink into that
directory. No root daemon, setuid binary, or automatic service activation is
installed. System packages disable the archive self-updater.

## Sign and verify locally

The reviewed public key is [peritus-release.asc](../keys/peritus-release.asc):

```text
UID: Peritus Release Signing <dollspacegay@gmail.com>
Fingerprint: C5FA A006 1C56 096C 0DB5 7E93 BC11 52DD CE40 9792
Expires: 2028-09-05
```

Keep the private key and its revocation certificate backed up securely outside
the repository. Neither belongs in a source archive, issue, build log, or chat.
Local signing forwards only GnuPG's restricted agent socket into an isolated,
network-disabled container; it does not export the private key. Unlock the key
through the local GnuPG pinentry prompt when required.

```sh
export PERITUS_SIGNING_FINGERPRINT=C5FAA0061C56096C0DB57E93BC1152DDCE409792
cargo xtask distro-sign
cargo xtask distro-verify
```

Signing copies the unsigned packages into a separate staged directory and
publishes `dist/signed/<format>` only after every signature succeeds. Unsigned
input remains intact. A signing failure never replaces an earlier signed set.

Debian packages receive embedded `debsigs` origin signatures. After that changes
the package bytes, all checksum sections are refreshed in dependency order and
`.dsc`, `.buildinfo`, and `.changes` are OpenPGP clear-signed. RPMs, including
source/debug RPMs, receive native `rpmsign` signatures. Both formats also have a
signed SHA-256 inventory and a detached-signed source/binary bundle.
These follow the native [Debian source package workflow](https://www.debian.org/doc/debian-policy/ch-source.html),
[debsigs format](https://manpages.debian.org/trixie/debsigs/debsigs.1p.en.html), and
[RPM signing interface](https://rpm.org/docs/6.1.x/man/rpmsign.1).

Verification uses only the public key. It checks complete checksum inventories,
Debian signed metadata closure and source/binary Lintian errors, native package signatures, changed-payload and
unsigned-package rejection, real APT/DNF installation, installed versions and
file integrity, package-manager update ownership through a real terminal, and
removal. These tests do not access provider accounts or start a benchmark.

Lintian checks the retained unsigned build whose hashes are bound by the signed
build record. Its Debian-archive submission policy rejects all extra underscore
members, including `debsigs`' `_gpgorigin`; the
[Debian binary format](https://manpages.debian.org/trixie/dpkg-dev/deb.5.en.html)
permits additional members after the payload. The signed files then undergo
`debsig-verify`, full signed-metadata checks, and actual APT installation. No
blanket suppression of Lintian errors or removal of package signatures is used.

Debian's ordinary `apt install ./file.deb` does **not** automatically enforce
embedded `debsigs` signatures. Verify the signed release bundle before local
installation, or use an explicitly installed `debsig-verify` trust policy.
This workflow publishes GitHub release assets; it does not create a hosted APT
or DNF repository, nor sign APT `InRelease` metadata.

## Tagged release workflow

The existing release workflow retains native Linux, macOS, and Windows archives
for x86-64 and ARM64, with their SBOM/provenance attestations. Four additional
matrix legs build and sign Debian/RPM packages for x86-64 and ARM64.
Each has separate image, compilation, packaging, and protected-signing jobs with the repository's
10-minute job ceiling. Actual hosted-runner timing still requires CI validation.

Before a first release:

1. Choose a non-placeholder `MAJOR.MINOR.PATCH` version and update the workspace,
   exact internal dependency versions, and lockfile together.
2. Set the repository variable `PERITUS_PACKAGE_MAINTAINER` to the approved
   `Name <email>` identity.
3. Create the GitHub environment `release-signing`, restrict it to release tags,
   and configure required reviewers. Environment protection is an operator
   setting, not something a workflow file can guarantee.
4. Explicitly provision `PERITUS_SIGNING_KEY` (armored private key) and
   `PERITUS_SIGNING_PASSPHRASE` as that environment's secrets through an approved
   secure channel. An unencrypted key requires an explicitly empty passphrase
   secret. For this repository the release owner authorized provisioning both
   protected environment secrets. Routine local signing still never exports a key.
5. Review and satisfy the separate [H4 release approval requirements](../../docs/h4-release-qualification.md),
   validate the native runner matrix, and authorize tagging/publication.

CI imports the supplied secret only into an ephemeral GnuPG home, checks the
reviewed fingerprint and exact tag/commit, signs, qualifies, then uploads to the
existing draft. Signing secrets are scoped to the signing step. The public key
asset has one matrix owner to prevent concurrent upload races. Draft completion
requires all six native archives/evidence sets, all four signed distribution
bundles, all four main native packages, the public key, and both bootstraps with
their checksums. A partial package matrix cannot complete staging.

The tag workflow ends with `cargo xtask release-stage` and always leaves the
release as a draft. Staging is not H4 approval. Qualify and retain the exact draft
bytes, complete H4, and obtain separate release-owner authorization before public
publication. No workflow or `xtask` command automatically publishes the draft.

Native package OpenPGP signatures and GitHub attestations are not Windows
Authenticode signatures or Apple Developer ID signatures/notarization. No
platform signing certificates or identities are assumed by this workflow.
