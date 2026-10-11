# peritus-tools-quality

`peritus-tools-quality` supplies the built-in `quality.discover` and `quality.run` tools. Discovery
combines explicitly supplied typed definitions with deterministic Cargo and Just surfaces from an
immutable C1 workspace. Discovered checks never become acceptance policy implicitly.

Runs use the same restricted C2/C3 path as shell execution and produce structured execution facts
plus candidate B2 gate-observation inputs. The crate never asserts snapshot freshness or final gate
acceptance, and infrastructure, parser, timeout, cancellation, or artifact failures cannot pass.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-tools-quality
```
