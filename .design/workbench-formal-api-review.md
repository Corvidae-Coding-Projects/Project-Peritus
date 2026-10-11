# Workbench ordinary-API checker repair

This repair models previously rejected source forms; it does not exclude tests, packages,
files, or public entry points from scanning. The accepted external expansions are tied to
the current locked `serde_json` 1.0.149, Tokio 1.53.1, and `tokio-macros` 2.7.2 dependencies.
Their local registry macro implementations were inspected before the policy change.

## Expansion boundaries

- Serde JSON `json!` constructs a `Value` expression through `json_internal!`. It does not
  generate caller-visible public API. Only the exact qualified `serde_json::json!` spelling
  or a direct, unaliased `use serde_json::json` import is accepted.
- Tokio `select!` generates a block-local output enum, owns its future tuple, polls pinned
  futures, then evaluates the selected user expression. Its generated module remains inside
  the expression block. Tokio `pin!` is limited here to its single-identifier arm: it moves
  the value into a fresh local, then shadows that local with a pin. Arbitrary unqualified,
  re-exported, raw-identifier, and nested namespace spellings are not accepted.
- `#[tokio::test]` is limited to the argument-free default-runtime form. Custom runtime,
  crate, and scheduling options remain rejected. The original function and every body token
  are still scanned for public contracts.
- `println!`, `writeln!`, and `unreachable!` are the standard expression/termination variants
  of the already modeled formatting/write/panic families. Their arguments remain visible.

The scanner never skips an accepted macro payload. Regression tests deliberately place public
preconditions and an unknown nested macro inside otherwise accepted expansions and require
their rejection. Imported or locally declared namespaces cannot impersonate the pinned crates;
aliases, external crate aliases, custom derives, globs, and unreviewed expansion names still fail.
Ordinary function imports named `select` remain ordinary functions, not macro exemptions.

## Persistence metadata

Grouped Serde imports accept only the flat reviewed Deserialize/Serialize names, without aliases
or nested imports. A closed metadata grammar supports default values, unknown-field rejection,
reviewed case conversion, simple literal field/tag names, and the exact omission predicates
already used by the persisted control records. Duplicate keys, custom conversions, flattening,
deserializer hooks, custom defaults, and arbitrary omission callbacks remain rejected.
These forms alter serialized representation, not public Rust contract visibility. Existing
control/journal round-trip and legacy decoding tests remain the behavioral qualification.

Native test ignores require an explicit non-empty reason. Enum default markers accept no
arguments. Neither annotation removes its function/type from the source scan or turns an
unexecuted native test into a passing result.

## Verification checkpoint

The focused API checker suite passed 39 tests, including three new adversarial matrices.
The real ordinary-API gate passed with 3287 formal-boundary files and 14466 ordinary-safe
executable entry points. Strict xtask Clippy also passed. Full local/trust gate and independent
proof-impact qualification remain separately tracked; these results do not claim their approval.
