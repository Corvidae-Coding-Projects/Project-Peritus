//! Human-readable package and release command inventory.

pub(super) const HELP: &str = "Peritus packaging and release tool

Usage: cargo xtask <command>

Commands:
  product-package          Build a host-native Peritus package in dist/
  product-install          Build and install Peritus for the current user
  release-create           Create a tag-bound draft GitHub release
  release-package-stage    Build, archive, and checksum this host's package
  release-package-assemble Assemble a package from native release binaries
  release-native-stage     Complete the native draft with tag-bound installers
  release-stage            Complete the release draft without publishing it
  release-rebuild-record   Retain native assembly observations
  release-daemon-library   Build the native daemon library
  release-daemon-binary    Build the native daemon binary
  release-cli-library      Build the native CLI library
  release-cli-binary       Build the native CLI binary
  release-windows-binary   Build a selected native Windows binary
  distro-image             Build a pinned package-builder image
  distro-image-save        Retain the package-builder image
  distro-image-restore     Restore the package-builder image
  distro-build             Build native distribution packages offline
  distro-compile           Retain native distribution compilation
  distro-package-compiled  Package the retained compilation
  distro-sign              Sign distribution packages
  distro-sign-ci           Sign with provisioned CI credentials
  distro-verify            Authenticate packages and inspect install/remove behavior
  distro-upload            Upload packages to an existing release draft
  help                     Print this help
";
