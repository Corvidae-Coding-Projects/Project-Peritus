//! Immutable content slices with scope-bound integrity, published in the admission transaction.

use super::{Error, digest, problem};
use peritus_app_protocol::{IMPROVEMENT_TEXT_CHUNK_BYTES, ImprovementTextPage, ImprovementTextQuery};
use rusqlite::{OptionalExtension, Transaction, params};

pub(super) fn write(transaction: &Transaction<'_>, workspace: &[u8; 16], candidate: &[u8; 32], source: &[u8; 16], source_digest: [u8; 32], text: &str) -> Result<(), Error> {
    let mut start = 0;
    while start < text.len() {
        let mut end = start.saturating_add(IMPROVEMENT_TEXT_CHUNK_BYTES).min(text.len());
        while !text.is_char_boundary(end) { end -= 1; }
        let offset = u64::try_from(start).map_err(problem)?;
        let next = u64::try_from(end).map_err(problem)?;
        let body = &text.as_bytes()[start..end];
        transaction.execute(
            "INSERT INTO improvement_text_chunks(workspace,candidate,source,offset,next,source_digest,chunk_digest,body) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![workspace, candidate, source, offset, next, source_digest,
                chunk_digest(workspace, candidate, source, &source_digest, offset, next, body), body],
        ).map_err(problem)?;
        start = end;
    }
    Ok(())
}

pub(super) fn read(transaction: &Transaction<'_>, query: ImprovementTextQuery) -> Result<ImprovementTextPage, Error> {
    if query.offset() == query.source().bytes() {
        return ImprovementTextPage::new(query, String::new(), None).map_err(problem);
    }
    let source = query.run().map_or([0; 16], peritus_types::RunId::into_bytes);
    let value: Option<(u64, [u8; 32], [u8; 32], Vec<u8>)> = transaction.query_row(
        "SELECT next,source_digest,chunk_digest,body FROM improvement_text_chunks WHERE workspace=?1 AND candidate=?2 AND source=?3 AND offset=?4",
        params![query.workspace().as_bytes(), query.candidate().as_bytes(), source, query.offset()],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    ).optional().map_err(problem)?;
    let (next, source_digest, observed, body) = value.ok_or_else(|| problem("source offset is not a retained chunk boundary"))?;
    if source_digest != query.source().digest().into_bytes()
        || next <= query.offset() || next > query.source().bytes()
        || next.checked_sub(query.offset()) != Some(u64::try_from(body.len()).map_err(problem)?)
        || body.len() > IMPROVEMENT_TEXT_CHUNK_BYTES
        || observed != chunk_digest(query.workspace().as_bytes(), query.candidate().as_bytes(), &source, &source_digest, query.offset(), next, &body)
    { return Err(problem("improvement source slice integrity mismatch")); }
    ImprovementTextPage::new(query, String::from_utf8(body).map_err(problem)?,
        (next < query.source().bytes()).then_some(next)).map_err(problem)
}

fn chunk_digest(workspace: &[u8; 16], candidate: &[u8; 32], source: &[u8; 16], source_digest: &[u8; 32], offset: u64, next: u64, body: &[u8]) -> [u8; 32] {
    digest(&[b"peritus.improvement.text-slice.v1", workspace, candidate, source, source_digest,
        &offset.to_le_bytes(), &next.to_le_bytes(), body])
}
