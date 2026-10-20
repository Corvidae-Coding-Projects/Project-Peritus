# OpenCode runtime selected through WSL interop

This replay uses the production OpenCode discovery and catalog entry points and checks the
pinned executable against an independent native-host oracle. It reproduces the reported
`PERITUS-PROVIDER-CORE-009 model_catalog` failure: a Windows OpenCode installation reached
through WSL interop answers the pinned version probe but cannot run the metadata command that
discovery needs.

```sh
python3 docs/testing/reproducers/opencode-wsl-interop/replay.py \
  --output /absolute/path/to/new-owned-output
```

The output directory must not exist. The runner builds a real public-library consumer with two
Cargo jobs using the checkout's cached dependency versions, runs it, and records the pinned
runtime, its nativity, and the catalog outcome in the output directory.

Run this from a WSL distribution with interop enabled and a Windows OpenCode installation on
`PATH`; the defect does not exist on hosts that only offer native executables. A restricted
sandbox that blocks WSL interop hides the defect, because the foreign candidate then fails the
version probe for an unrelated reason.

Expected results:

| Program | Expectation |
| --- | --- |
| Pre-fix | `runtime /mnt/<drive>/...`, `nativity foreign`, `catalog PERITUS-PROVIDER-CORE-009: OpenCode did not return a complete service model catalog...`, exit 1 |
| Fixed, no native OpenCode 1.19.0 | `runtime none`, exit 0 |
| Fixed, native OpenCode 1.19.0 on `PATH` after the Windows entry | `nativity native`, `catalog <n> advertised models`, exit 0 |

The consumer uses a disposable placeholder credential. Only service metadata is requested; no
inference, no stored key, and no account state are involved. [observed-baseline.txt](observed-baseline.txt)
retains the original failing observations. See the
[campaign report](../../opencode-wsl-interop-real-testing-2026-10-20.md) for the complete
before/after evidence and the exact commands.
