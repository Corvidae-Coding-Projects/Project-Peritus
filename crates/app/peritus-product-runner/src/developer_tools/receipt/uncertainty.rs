//! Unknown command inspection, acknowledgement, and cross-invocation fencing.

use std::{fs, io::Write as _};

use super::*;

fn record_inactive_command(path: &Path, scope: &str, call: &CompletedToolCall) {
    let owner = NativeCommandOwner {
        source_run: RunId::new([1; 16]).expect("source run ID"),
        execution_run: RunId::new([2; 16]).expect("process run ID"),
        action: ActionId::new([3; 16]).expect("action ID"),
        process: ProcessId::new([4; 16]).expect("process ID"),
    };
    let mut ledger = EffectReceiptLedger::new(path.to_path_buf(), scope.to_owned());
    assert!(matches!(ledger.begin(call).expect("start command"), ReceiptDecision::Execute));
    ledger
        .bind_native_command_owner(call.id().expose_for_wire(), owner)
        .expect("persist exact command owner");
    ledger
        .reconcile_native_command_owner(
            owner,
            peritus_process::RecoveryDisposition::AbsentUnobserved,
            &Value::Null,
        )
        .expect("persist exact inactive owner observation");
}
