#!/usr/bin/env bash
set -euo pipefail
source_path=crates/orchestration/peritus-scheduler/src/reducer/reconstruction.rs
saved_dir=$(mktemp -d /tmp/peritus-reconstruction-probe.XXXXXX)
cp -- "$source_path" "$saved_dir/original.rs"
restore_source() {
    cp -- "$saved_dir/original.rs" "$source_path"
}
trap restore_source EXIT INT TERM
[[ $(rg -c 'if \*descendants \{' "$source_path") == 1 ]]
perl -0pi -e 's/if \*descendants \{/if !*descendants {/ or die "mutation target missing\n"' "$source_path"
diff -u --label a/crates/orchestration/peritus-scheduler/src/reducer/reconstruction.rs --label b/crates/orchestration/peritus-scheduler/src/reducer/reconstruction.rs "$saved_dir/original.rs" "$source_path" > target/gap3-evidence/206-reconstruction-mutation.diff || [[ $? == 1 ]]
set +e
CARGO_BUILD_JOBS=1 CCACHE_DISABLE=1 RUST_TEST_THREADS=1 taskset -c 0,1 cargo verus verify --package peritus-scheduler --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20 --multiple-errors 10 > target/gap3-evidence/206-reconstruction-mutation.log 2>&1
probe_status=$?
set -e
restore_source
cmp -- "$saved_dir/original.rs" "$source_path"
trap - EXIT INT TERM
printf 'Verus exit: %s\nSource restoration: byte-identical\n' "$probe_status" > target/gap3-evidence/206-reconstruction-mutation-restoration.log
[[ $probe_status == 101 ]]
rg -q 'postcondition not satisfied' target/gap3-evidence/206-reconstruction-mutation.log
