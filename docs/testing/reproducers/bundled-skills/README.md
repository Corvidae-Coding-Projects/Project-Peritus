# Automatic skill bundle replay

Run from the repository root with the supported Rust toolchain and Python 3.11 or newer:

```sh
python3 docs/testing/reproducers/bundled-skills/replay.py
```

This builds a native consumer of `peritus_product_runner::bundled_skills` and calls the actual
catalog and production page readers. Expected resource names and bytes come independently from
the canonical source files. The consumer emits observations; the Python driver checks them.
No unit-test target, model provider, user daemon, or installed binary is involved.

The replay creates two clearly named disposable packages under `skills/`, refuses to replace
existing paths, and removes its own fixtures in a `finally` block. It rebuilds to exercise package
addition/removal and file addition/update/deletion. It also moves one fixture source outside the
tree temporarily and reads its already compiled resources through a fresh process.

Seed `20261020` chooses resource payloads and page budgets. Scenarios cover metadata descriptions,
plain Markdown, CRLF, nested and quoted Unicode paths, arbitrary extensions, binary data, empty
files, pagination, SHA-256 identities, EOF, and invalid names/offsets/budgets. Helper files remain
inert. Full observations, build logs and the summary are retained outside the repository under
`~/.cache/peritus-skills-20261020/`; override with `PERITUS_SKILL_REPLAY_DIR` if needed.

Scope: compiled native catalog and exact readers, plus Cargo's real rebuild behavior. This replay
does not establish provider selection quality or UI behavior.
