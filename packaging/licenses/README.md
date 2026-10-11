# Supplemental upstream license notices

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
