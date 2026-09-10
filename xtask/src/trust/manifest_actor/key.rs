//! Strict Crosslink signing-key identity binding.

use crate::trust::manifest_actor_model::{ActorEntry, ActorProvenanceEntry};
use base64::Engine as _;
use sha2::{Digest, Sha256};

pub(super) fn matches(actor: &ActorEntry, provenance: &ActorProvenanceEntry) -> bool {
    let Some(public_key) = provenance.public_key.as_deref() else { return false };
    let Some(allowed_signer) = provenance.allowed_signer.as_deref() else { return false };
    let public: Vec<_> = public_key.split_ascii_whitespace().collect();
    let signer: Vec<_> = allowed_signer.split_ascii_whitespace().collect();
    if public.len() != 3
        || signer.len() != 4
        || public[0] != "ssh-ed25519"
        || signer[1] != public[0]
        || signer[2] != public[1]
        || signer[3] != public[2]
    {
        return false;
    }
    let Some((agent_id, _machine)) = key_comment_identity(public[2]) else { return false };
    if signer[0] != format!("{agent_id}@crosslink") {
        return false;
    }
    let Ok(blob) = base64::engine::general_purpose::STANDARD.decode(public[1]) else {
        return false;
    };
    if !valid_ed25519_blob(&blob)
        || base64::engine::general_purpose::STANDARD.encode(&blob) != public[1]
    {
        return false;
    }
    let digest = Sha256::digest(blob);
    let encoded = base64::engine::general_purpose::STANDARD_NO_PAD.encode(digest);
    actor.principal == format!("SHA256:{encoded}")
}

fn key_comment_identity(comment: &str) -> Option<(&str, &str)> {
    let identity = comment.strip_prefix("crosslink-agent:")?;
    let (agent_id, machine) = identity.split_once('@')?;
    if identity.matches('@').count() != 1
        || !normal_component(agent_id, 64, |byte| byte == b'-' || byte == b'_')
        || machine.len() > 253
        || !machine.split('.').all(|label| normal_component(label, 63, |byte| byte == b'-'))
    {
        return None;
    }
    Some((agent_id, machine))
}

fn normal_component(
    value: &str,
    maximum: usize,
    permitted_punctuation: impl Fn(u8) -> bool,
) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value.is_ascii()
        && value.as_bytes().first().is_some_and(u8::is_ascii_alphanumeric)
        && value.as_bytes().last().is_some_and(u8::is_ascii_alphanumeric)
        && value.bytes().all(|byte| byte.is_ascii_alphanumeric() || permitted_punctuation(byte))
}

fn valid_ed25519_blob(blob: &[u8]) -> bool {
    blob.len() == 51
        && blob.get(..4) == Some(&11_u32.to_be_bytes())
        && blob.get(4..15) == Some(b"ssh-ed25519")
        && blob.get(15..19) == Some(&32_u32.to_be_bytes())
}

#[cfg(test)]
mod tests {
    use super::matches;
    use crate::trust::manifest_actor_model::{
        ActorEntry, ActorKind, ActorMode, ActorProvenanceEntry, ActorProvenanceRef, ActorRole,
    };

    const PRINCIPAL: &str = "SHA256:IeCr62P7gzB0M5HnHg0Ncs6fEmNgIObFyetKV6DXuCc";
    const KEY: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIAZPdV4lzU+Eo5usUd7qoun0ZvzCeSvzn4hYWylAgN1T";
    const COMMENT: &str = "crosslink-agent:krvx@dollspace-laptop";

    #[test]
    fn current_krvx_identity_is_derived_from_its_checked_key_comment() {
        let actor = actor();
        let provenance = provenance(COMMENT, "krvx@crosslink", COMMENT);
        assert!(matches(&actor, &provenance));
    }

    #[test]
    fn wrong_signer_identity_and_key_comment_are_rejected() {
        let actor = actor();
        assert!(!matches(&actor, &provenance(COMMENT, "6ME5@crosslink", COMMENT)));
        assert!(!matches(&actor, &provenance("fixture-owner", "krvx@crosslink", "fixture-owner")));
        assert!(!matches(
            &actor,
            &provenance(COMMENT, "krvx@crosslink", "crosslink-agent:krvx@other-machine")
        ));
    }

    fn actor() -> ActorEntry {
        ActorEntry {
            id: "ACTOR-0003".to_owned(),
            kind: ActorKind::CrosslinkAgent,
            principal: PRINCIPAL.to_owned(),
            display_name: "fixture".to_owned(),
            roles: vec![ActorRole::Owner],
            provenance: ActorProvenanceRef {
                record_path: "verification/actor-provenance.json".to_owned(),
                record_sha256: "a".repeat(64),
            },
        }
    }

    fn provenance(
        key_comment: &str,
        signer_id: &str,
        signer_comment: &str,
    ) -> ActorProvenanceEntry {
        ActorProvenanceEntry {
            actor_id: "ACTOR-0003".to_owned(),
            kind: ActorKind::CrosslinkAgent,
            principal: PRINCIPAL.to_owned(),
            repository: "Corvidae-Coding-Projects/Project-Peritus".to_owned(),
            issue: 66,
            issue_created_at: "2026-09-09T03:39:44Z".to_owned(),
            session: 23,
            task: "/root".to_owned(),
            mode: ActorMode::Implementation,
            model: None,
            reasoning_effort: None,
            public_key: Some(format!("ssh-ed25519 {KEY} {key_comment}")),
            allowed_signer: Some(format!("{signer_id} ssh-ed25519 {KEY} {signer_comment}")),
            record_locators: vec![
                "embedded:allowed-signer".to_owned(),
                "embedded:public-key".to_owned(),
            ],
        }
    }
}
