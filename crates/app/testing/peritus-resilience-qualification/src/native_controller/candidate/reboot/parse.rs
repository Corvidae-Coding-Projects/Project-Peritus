//! Closed parsing for guest-produced reboot checkpoints and recovery facts.

use std::path::Path;

use crate::native_controller::request::RebootPhase;

const EFFECT_ROOT: &str = "/var/lib/peritus-h1/state/outbox-crash-qualification-v1/";

pub(super) struct StageFacts {
    pub(super) effect_path: String,
    pub(super) claim_fence: u64,
    pub(super) external_effects: u64,
}

pub(super) struct RecoveryFacts {
    pub(super) destination_reconciled: bool,
    pub(super) external_effects: u64,
    pub(super) duplicate_effects: u64,
    pub(super) exact_fence_acknowledged: bool,
    pub(super) pending_claims: u64,
}

pub(super) fn stage(
    line: &str,
    phase: RebootPhase,
    reconciliation: bool,
) -> Result<StageFacts, Box<dyn std::error::Error>> {
    let prefix = if reconciliation {
        "peritus-qualification reboot-reconciliation-stage "
    } else {
        "peritus-qualification reboot-stage "
    };
    let fields = fields(line, prefix, 4)?;
    if value(fields[0], "phase")? != phase.code() {
        return Err("guest reboot checkpoint returned the wrong phase".into());
    }
    let effect_path = value(fields[1], "effect_path")?;
    validate_effect_path(effect_path)?;
    let claim_fence = number(fields[2], "claim_fence")?;
    let external_effects = number(fields[3], "external_effects")?;
    if claim_fence == 0 || external_effects > 1 {
        return Err("guest reboot checkpoint returned impossible effect facts".into());
    }
    Ok(StageFacts { effect_path: effect_path.to_owned(), claim_fence, external_effects })
}

pub(super) fn recovery(
    line: &str,
    phase: RebootPhase,
) -> Result<RecoveryFacts, Box<dyn std::error::Error>> {
    let fields = fields(line, "peritus-qualification reboot-recover ", 6)?;
    if value(fields[0], "phase")? != phase.code() {
        return Err("guest reboot recovery returned the wrong phase".into());
    }
    let facts = RecoveryFacts {
        destination_reconciled: boolean(fields[1], "destination_reconciled")?,
        external_effects: number(fields[2], "external_effects")?,
        duplicate_effects: number(fields[3], "duplicate_effects")?,
        exact_fence_acknowledged: boolean(fields[4], "exact_fence_acknowledged")?,
        pending_claims: number(fields[5], "pending_claims")?,
    };
    if !facts.destination_reconciled
        || facts.external_effects != 1
        || facts.duplicate_effects != 0
        || !facts.exact_fence_acknowledged
        || facts.pending_claims != 0
    {
        return Err("guest reboot recovery did not settle the exact effect once".into());
    }
    Ok(facts)
}

pub(super) fn boot_id(value: &str) -> Result<String, Box<dyn std::error::Error>> {
    let value = value.trim_end_matches(['\r', '\n']);
    let valid = value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) { byte == b'-' } else { byte.is_ascii_hexdigit() }
        });
    if valid { Ok(value.to_ascii_lowercase()) } else { Err("guest boot ID is malformed".into()) }
}

fn fields<'a>(
    line: &'a str,
    prefix: &str,
    expected: usize,
) -> Result<Vec<&'a str>, Box<dyn std::error::Error>> {
    let values = line
        .strip_prefix(prefix)
        .ok_or("guest returned an unknown reboot observation")?
        .split_whitespace()
        .collect::<Vec<_>>();
    if values.len() != expected {
        return Err("guest reboot observation has the wrong field count".into());
    }
    Ok(values)
}

fn value<'a>(field: &'a str, name: &str) -> Result<&'a str, Box<dyn std::error::Error>> {
    let (actual, value) = field.split_once('=').ok_or("malformed guest reboot field")?;
    if actual == name && !value.is_empty() {
        Ok(value)
    } else {
        Err(format!("expected guest reboot field {name}").into())
    }
}

fn number(field: &str, name: &str) -> Result<u64, Box<dyn std::error::Error>> {
    value(field, name)?.parse::<u64>().map_err(Into::into)
}

fn boolean(field: &str, name: &str) -> Result<bool, Box<dyn std::error::Error>> {
    match value(field, name)? {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("guest reboot field {name} is not boolean").into()),
    }
}

fn validate_effect_path(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let Some(name) = path.strip_prefix(EFFECT_ROOT) else {
        return Err("guest reboot checkpoint named an effect outside its state root".into());
    };
    let identity = name.strip_prefix("delivery-").and_then(|value| value.strip_suffix(".effect"));
    if Path::new(name).components().count() != 1
        || identity.is_none_or(|value| {
            value.len() != 32
                || !value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
    {
        return Err("guest reboot checkpoint named a malformed effect".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{boot_id, recovery, stage};
    use crate::native_controller::request::RebootPhase;

    #[test]
    fn accepts_closed_guest_documents() {
        let checkpoint = "peritus-qualification reboot-stage phase=durable-before-ack effect_path=/var/lib/peritus-h1/state/outbox-crash-qualification-v1/delivery-00000000000000000000000000000001.effect claim_fence=1 external_effects=1";
        let parsed = stage(checkpoint, RebootPhase::DurableBeforeAck, false).unwrap();
        assert_eq!(parsed.claim_fence, 1);
        let observation = "peritus-qualification reboot-recover phase=durable-before-ack destination_reconciled=true external_effects=1 duplicate_effects=0 exact_fence_acknowledged=true pending_claims=0";
        assert!(recovery(observation, RebootPhase::DurableBeforeAck).is_ok());
        assert!(boot_id("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee").is_ok());
    }
}
