//! Domain-separated event and batch hash construction.

use crate::{AggregateKey, EventDraft, StoreId};
use peritus_codec::sha256;
use peritus_types::{CommandId, Sha256Digest};

const EVENT_HASH_DOMAIN: &[u8] = b"peritus.journal.event.v1\0";
const BATCH_HASH_DOMAIN: &[u8] = b"peritus.journal.batch.v1\0";
const JOURNAL_HEAD_DOMAIN: &[u8] = b"peritus.journal.head.v1\0";

pub fn event_hash(
    draft: &EventDraft,
    previous_hash: Sha256Digest,
    command_id: CommandId,
) -> Sha256Digest {
    let mut input = Vec::with_capacity(
        EVENT_HASH_DOMAIN.len()
            + 2
            + 16
            + 8
            + 16
            + 1
            + 16
            + 32
            + 16
            + 32
            + 32
            + 4
            + draft.causal_parents().len() * EventIdLength::VALUE,
    );
    input.extend_from_slice(EVENT_HASH_DOMAIN);
    input.extend_from_slice(&draft.aggregate().kind().hash_tag().to_be_bytes());
    input.extend_from_slice(draft.aggregate().id().as_bytes());
    input.extend_from_slice(&draft.sequence().get().to_be_bytes());
    input.extend_from_slice(draft.event_id().as_bytes());
    if let Some(previous) = draft.previous_event_id() {
        input.push(1);
        input.extend_from_slice(previous.as_bytes());
    } else {
        input.push(0);
        input.extend_from_slice(&[0; 16]);
    }
    input.extend_from_slice(previous_hash.as_bytes());
    input.extend_from_slice(command_id.as_bytes());
    input.extend_from_slice(draft.frame().digest().as_bytes());
    input.extend_from_slice(draft.revision_digest().as_bytes());
    let causal_count = u32::try_from(draft.causal_parents().len())
        .expect("validated causal-parent bound fits u32");
    input.extend_from_slice(&causal_count.to_be_bytes());
    for parent in draft.causal_parents() {
        input.extend_from_slice(parent.as_bytes());
    }
    sha256(&input)
}

pub fn batch_hash(
    store_id: StoreId,
    command_id: CommandId,
    request_digest: Sha256Digest,
    event_hashes: impl IntoIterator<Item = Sha256Digest>,
    event_count: usize,
    artifact_digests: impl IntoIterator<Item = Sha256Digest>,
    artifact_count: usize,
) -> Sha256Digest {
    let mut input = Vec::with_capacity(
        BATCH_HASH_DOMAIN.len() + 16 + 16 + 32 + 4 + event_count * 32 + 4 + artifact_count * 32,
    );
    input.extend_from_slice(BATCH_HASH_DOMAIN);
    input.extend_from_slice(store_id.as_bytes());
    input.extend_from_slice(command_id.as_bytes());
    input.extend_from_slice(request_digest.as_bytes());
    let event_count = u32::try_from(event_count).expect("validated batch bound fits u32");
    input.extend_from_slice(&event_count.to_be_bytes());
    for hash in event_hashes {
        input.extend_from_slice(hash.as_bytes());
    }
    let artifact_count = u32::try_from(artifact_count).expect("validated artifact bound fits u32");
    input.extend_from_slice(&artifact_count.to_be_bytes());
    for digest in artifact_digests {
        input.extend_from_slice(digest.as_bytes());
    }
    sha256(&input)
}

pub fn journal_head_hash(
    store_id: StoreId,
    last_position: u64,
    heads: impl IntoIterator<Item = (AggregateKey, u64, Sha256Digest)>,
    count: usize,
) -> Sha256Digest {
    let mut input = Vec::with_capacity(JOURNAL_HEAD_DOMAIN.len() + 16 + 8 + 4 + count * 58);
    input.extend_from_slice(JOURNAL_HEAD_DOMAIN);
    input.extend_from_slice(store_id.as_bytes());
    input.extend_from_slice(&last_position.to_be_bytes());
    let count = u32::try_from(count).expect("SQLite aggregate count fits u32");
    input.extend_from_slice(&count.to_be_bytes());
    for (key, sequence, hash) in heads {
        input.extend_from_slice(&key.kind().hash_tag().to_be_bytes());
        input.extend_from_slice(key.id().as_bytes());
        input.extend_from_slice(&sequence.to_be_bytes());
        input.extend_from_slice(hash.as_bytes());
    }
    sha256(&input)
}

struct EventIdLength;

impl EventIdLength {
    const VALUE: usize = 16;
}
