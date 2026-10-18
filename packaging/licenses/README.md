# Supplemental upstream license notices

## Folder guide

Supplemental upstream license notices shipped with source bundles. Preserve the notices and their exact scope.

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |
| [declared-MIT.txt](declared-MIT.txt) | Standard MIT text for exact-version manifest declarations whose crates omit license files; provenance is explicit. |
| [rust-phf-MIT.txt](rust-phf-MIT.txt) | Exact-revision rust-phf family upstream MIT notice. |
| [servo-selectors-MPL.txt](servo-selectors-MPL.txt) | Standard MPL 2.0 text for the selectors parser's declared license. |
| [valuable-MIT.txt](valuable-MIT.txt) | Exact-revision valuable upstream MIT notice. |
| [anes-MIT.txt](anes-MIT.txt) | Upstream license/notice text for anes MIT. |
| [convert-case-MIT.txt](convert-case-MIT.txt) | Upstream license/notice text for convert case MIT. |
| [jni-MIT.txt](jni-MIT.txt) | Upstream license/notice text for jni MIT. |
| [verus-MIT.txt](verus-MIT.txt) | Upstream license/notice text for verus MIT. |

These notices accompany the locked source bundle because some published crates omit
their upstream license files. They do not modify the vendored Cargo checksums.
The generator also retains declared authors and licenses from every crate manifest.

| Notice | Source and scope |
| --- | --- |
| `anes-MIT.txt` | [ANES 0.1.6 source revision](https://github.com/qwandor/anes-rs/blob/378ac7461ecac29fde9e10df7b08359d1315a9ad/LICENSE-MIT). Upstream moved from `zrzka` to `qwandor`; the original README retains contributor copyrights. |
| `convert-case-MIT.txt` | [Upstream license blob c0ce8f0](https://api.github.com/repos/rutrum/convert-case/git/blobs/c0ce8f0a3b7351d84a1420d11a8c045d333870a3). Version 0.4.0 already declares MIT in its manifest but has no license file, even in its source revision. This later upstream notice is identified as later, not misrepresented as a file shipped in 0.4.0. |
| `jni-MIT.txt` | [JNI 0.22.4 source](https://github.com/jni-rs/jni-rs/blob/5ae9458a4ec44c5318f37ddc7569c1d4ae8a69e7/LICENSE-MIT); the same notice exists at the JNI macros revision `33045a124105c939d1e2cbdcb5a39e5d868ffa03`. |
| `verus-MIT.txt` | [Exact workspace-pinned Verus source](https://github.com/verus-lang/verus/blob/92f466f247f45128c630d1c843fd6e27d2115587/LICENSE). Covers the four sibling crates whose individual vendor directories omit the repository-level license. |

For `jni-sys-macros`, the winapi import-library crates, and the rustls Android
companion crate, the generator includes the license from the corresponding locked
parent project's vendored crate. `r-efi` embeds its full MIT grant and copyrights in
`AUTHORS`; `yaml-rust2` uses `MIT-LICENSE` and `Apache-LICENSE` filenames.

Adding or upgrading a dependency requires checking this inventory again. An
unrecognized missing license notice fails source preparation instead of being omitted.

The built-in Rust search dependency closure adds rust-phf's exact revision
`bed01538ae576876f11189d541875d228acef9e8` and valuable's exact revision
`9efc29b6e58cef28f6566a47aa7e142a55fead77`. Selectors declares MPL-2.0 in
its manifest and source headers; its supplemental file supplies the standard MPL
text from a later upstream copy. `declared-MIT.txt` explicitly supplies standard
MIT terms for the listed exact-version crates' MIT declarations; it is not
represented as a recovered upstream license file. Original manifests, authors
and source headers remain in the complete source bundle. The version-specific
mapping also retains r-efi 5.3.0's embedded license from AUTHORS.
