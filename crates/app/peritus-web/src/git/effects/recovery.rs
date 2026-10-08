//! Activation, independent cancellation, and restart reconciliation.

use super::{
    EffectResult, EffectState, Phase, POLL_INTERVAL, Recovery, Request, StoreBinding, observe,
    output, platform, read_json, read_optional, read_state, validate_request, validate_state,
};
use crate::{
    error::{Result, problem, uncertain},
    state::save,
};
use peritus_process::ProbeObservation;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub(super) async fn await_activation(
    directory: PathBuf,
    request: Request,
    mut child: tokio::process::Child,
) -> Result<Value> {
    loop {
        let state = match read_state(&directory) {
            Ok(state) => state,
            Err(error) => {
                stop_launched(&mut child).await;
                return Err(uncertain(format!(
                    "The Git owner record could not be read after launch: {}",
                    error.0,
                )));
            }
        };
        if let Some(state) = state {
            if let Err(error) = validate_state(&request, &state) {
                stop_launched(&mut child).await;
                return Err(error);
            }
            if let Err(error) = save(&directory.join("activate"), request.descriptor.as_bytes()) {
                stop_launched(&mut child).await;
                return Err(uncertain(format!(
                    "The Git activation receipt could not be published: {}",
                    error.0,
                )));
            }
            tokio::spawn(async move {
                let _ = child.wait().await;
            });
            return await_state(&directory, &request, state).await;
        }
        let observed = match child.try_wait() {
            Ok(observed) => observed,
            Err(error) => {
                stop_launched(&mut child).await;
                return Err(uncertain(format!(
                    "The launched Git owner could not be observed: {error}",
                )));
            }
        };
        if let Some(status) = observed {
            return Err(uncertain(format!(
                "The Git effect owner stopped before publishing a terminal receipt ({status})",
            )));
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

async fn stop_launched(child: &mut tokio::process::Child) {
    let _ = child.kill().await;
    let _ = child.wait().await;
}

pub(super) async fn await_state(
    directory: &Path,
    request: &Request,
    mut state: EffectState,
) -> Result<Value> {
    loop {
        validate_state(request, &state)?;
        if state.phase == Phase::Completed {
            let result = state
                .result
                .as_ref()
                .ok_or_else(|| problem("A completed Git effect has no result"))?;
            let directory = directory.to_owned();
            let request = request.clone();
            let result = result.clone();
            return tokio::task::spawn_blocking(move || {
                completed_value(&directory, &request, &result)
            })
            .await
            .map_err(problem)?;
        }
        let observation = observe(&state.owner);
        match observation {
            Ok(ProbeObservation::ExactLive) => {}
            Ok(ProbeObservation::ExactAbsent) => {
                if let Some(latest) = terminal_after_owner_change(directory, request)? {
                    state = latest;
                    continue;
                }
                return Err(uncertain(if directory.join("cancel").is_file() {
                    "The Git owner stopped after cancellation without publishing a terminal receipt; inspect the repository before another mutation"
                } else {
                    "The Git owner stopped without publishing a terminal receipt; inspect the repository before another mutation"
                }));
            }
            Ok(ProbeObservation::Mismatched | ProbeObservation::Unverifiable) | Err(_) => {
                if let Some(latest) = terminal_after_owner_change(directory, request)? {
                    state = latest;
                    continue;
                }
                return Err(uncertain(
                    "The retained Git owner identity cannot be verified; inspect the repository before another mutation",
                ));
            }
        }
        tokio::time::sleep(POLL_INTERVAL).await;
        state = read_state(directory)
            .map_err(|error| uncertain(format!("The Git owner state could not be refreshed: {}", error.0)))?
            .ok_or_else(|| uncertain("The durable Git effect record disappeared"))?;
    }
}

fn terminal_after_owner_change(
    directory: &Path,
    request: &Request,
) -> Result<Option<EffectState>> {
    let Some(state) = read_state(directory).map_err(|error| {
        uncertain(format!("The Git terminal receipt could not be read: {}", error.0))
    })? else {
        return Ok(None);
    };
    validate_state(request, &state)?;
    Ok((state.phase == Phase::Completed).then_some(state))
}

pub(super) fn cancel(
    store: &StoreBinding,
    operation: &str,
    prepared: Option<&Value>,
) -> Result<Value> {
    let directory = store.directory(operation);
    let request: Request = read_json(&directory.join("request.json"))?;
    validate_request(store, &request, prepared)?;
    if request.operation != operation {
        return Err(uncertain("The Git effect request belongs to a different operation"));
    }
    if let Some(state) = read_state(&directory)? {
        validate_state(&request, &state)?;
        if state.phase == Phase::Completed {
            return Ok(recover(store, operation, prepared)?.observation);
        }
    }
    save(&directory.join("cancel"), b"cancel-requested-v3\n")?;
    // The receipt-writing owner is the sole cancellation authority. It observes this durable
    // marker before start or from its retained-child wait loop; the gateway never races its reap
    // with an external PID/group signal.
    if let Some(state) = read_state(&directory)?
        && state.phase == Phase::Completed
    {
        return Ok(recover(store, operation, prepared)?.observation);
    }
    Ok(json!({
        "operation":operation,
        "cancelRequested":true,
        "uncertain":true,
        "note":"Cancellation is durable. Reconcile the sidecar's terminal receipt before another repository mutation."
    }))
}

pub(super) fn recover(
    store: &StoreBinding,
    operation: &str,
    prepared: Option<&Value>,
) -> Result<Recovery> {
    let directory = store.directory(operation);
    let Some(request) = read_optional::<Request>(&directory.join("request.json"))? else {
        return Ok(Recovery {
            result: None,
            observation: json!({"phase":"missing","uncertain":true}),
        });
    };
    validate_request(store, &request, prepared)?;
    if request.operation != operation {
        return Err(uncertain("The Git effect request belongs to a different operation"));
    }
    let Some(state) = read_state(&directory)? else {
        let launch = read_optional(&directory.join("launch.json"))?;
        if let Some(launch) = &launch {
            platform::validate_launcher_binding(launch)?;
        }
        let owner = launch.as_ref().map(observe).transpose()?;
        let phase = match owner {
            Some(ProbeObservation::ExactLive) => "launching",
            Some(ProbeObservation::ExactAbsent) => "stopped-without-receipt",
            Some(ProbeObservation::Mismatched | ProbeObservation::Unverifiable) => "unverifiable",
            None => "prepared",
        };
        return Ok(Recovery {
            result: None,
            observation: json!({"phase":phase,"uncertain":true}),
        });
    };
    validate_state(&request, &state)?;
    if state.phase == Phase::Completed {
        let result = state
            .result
            .as_ref()
            .ok_or_else(|| problem("A completed Git effect has no result"))?;
        let settled = match completed_value(&directory, &request, result) {
            Ok(value) => Some(value),
            Err(error) if error.1 => None,
            Err(error) => Some(json!({"error":error.0})),
        };
        let unresolved = settled.is_none();
        return Ok(Recovery {
            result: settled,
            observation: observation(&directory, &state, unresolved),
        });
    }
    let owner = observe(&state.owner)?;
    Ok(Recovery {
        result: None,
        observation: json!({
            "phase":format!("{:?}",state.phase).to_ascii_lowercase(),
            "owner":format!("{owner:?}"),
            "cancelRequested":directory.join("cancel").is_file(),
            "uncertain":owner != ProbeObservation::ExactLive,
            "activationPublished":directory.join("activate").is_file()
        }),
    })
}

fn completed_value(directory: &Path, request: &Request, result: &EffectResult) -> Result<Value> {
    if result.cancelled {
        return Err(if result.started {
            uncertain(format!(
                "The Git command was cancelled after it started{}; inspect the repository before another mutation",
                result
                    .internal_error
                    .as_deref()
                    .map(|detail| format!(": {detail}"))
                    .unwrap_or_default(),
            ))
        } else {
            problem("The Git command was cancelled before the sidecar started it")
        });
    }
    if let Some(error) = &result.internal_error {
        return Err(if result.started {
            uncertain(format!(
                "{error}. The Git effect started; inspect the repository before another mutation.",
            ))
        } else {
            problem(format!("Git could not start: {error}"))
        });
    }
    output::validate(directory, "stdout", &result.stdout)?;
    output::validate(directory, "stderr", &result.stderr)?;
    if result.status == Some(0) {
        return output::projection(directory, &request.operation, result);
    }
    let detail = output::error_detail(directory, result)?
        .unwrap_or_else(|| format!("Git exited without a status message ({:?})", result.status));
    if network_effect(&request.command.args) || mutating_effect(&request.command.args) {
        return Err(uncertain(format!(
            "{detail}\nGit did not confirm completion. Inspect local and remote state before another mutation.",
        )));
    }
    Err(problem(detail))
}

fn observation(directory: &Path, state: &EffectState, uncertain: bool) -> Value {
    let receipt = state.result.as_ref().map(|result| json!({
        "started":result.started,
        "cancelled":result.cancelled,
        "status":result.status,
        "stdout":{
            "bytes":result.stdout.bytes,
            "digest":result.stdout.digest,
            "complete":result.stdout.complete
        },
        "stderr":{
            "bytes":result.stderr.bytes,
            "digest":result.stderr.digest,
            "complete":result.stderr.complete
        }
    }));
    json!({
        "phase":format!("{:?}",state.phase).to_ascii_lowercase(),
        "cancelRequested":directory.join("cancel").is_file(),
        "uncertain":uncertain,
        "owner":{"pid":state.owner.pid,"completeContainment":state.owner.complete_containment},
        "receipt":receipt
    })
}

fn network_effect(args: &[String]) -> bool {
    args.first().is_some_and(|arg| ["fetch", "pull", "push"].contains(&arg.as_str()))
}

fn mutating_effect(args: &[String]) -> bool {
    args.iter().find(|arg| !arg.starts_with('-')).is_none_or(|command| command != "diff")
}
