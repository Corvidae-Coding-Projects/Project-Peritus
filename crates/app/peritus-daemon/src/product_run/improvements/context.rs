//! Run-bound projection of frozen improvement evidence into generic read-only context tools.

use super::{Error, ProductRunService, locked, store};
use peritus_product_runner::{ContextSource, ContextSourcePage, ContextSourceSlice};
use peritus_types::RunId;

impl ProductRunService {
    /// Returns a bounded catalog only when this exact product run owns an improvement
    /// reservation. Other run-specific source providers can handle `None` independently.
    pub(super) fn improvement_context_sources(
        &self,
        run: RunId,
        after: Option<u64>,
    ) -> Result<Option<ContextSourcePage>, Error> {
        let Some(reservation) = self.validated_improvement_reservation(run)? else {
            return Ok(None);
        };
        let page = locked(&self.inner.improvements)?.evaluation_sources(&reservation, after)?;
        let sources = page
            .sources()
            .iter()
            .copied()
            .map(|source| context_source(&reservation, source))
            .collect::<Result<Vec<_>, _>>()?;
        ContextSourcePage::new(after, sources, page.next())
            .map(Some)
            .map_err(|error| Error::internal("project improvement context sources", error))
    }

    /// Reads one exact retained chunk after revalidating both the run binding and source
    /// descriptor. No body is copied into the run record or accumulated across calls.
    pub(super) fn read_improvement_context_source(
        &self,
        run: RunId,
        ordinal: u64,
        offset: u64,
    ) -> Result<Option<ContextSourceSlice>, Error> {
        let Some(reservation) = self.validated_improvement_reservation(run)? else {
            return Ok(None);
        };
        let mut store = locked(&self.inner.improvements)?;
        let source = store.evaluation_source(&reservation, ordinal)?;
        let descriptor = context_source(&reservation, source)?;
        let page = store.text_page(source.query(&reservation, offset)?)?;
        ContextSourceSlice::new(
            &descriptor,
            offset,
            page.text().to_owned(),
            page.next(),
        )
        .map(Some)
        .map_err(|error| Error::internal("read improvement context source", error))
    }

    fn validated_improvement_reservation(
        &self,
        run: RunId,
    ) -> Result<Option<store::Reservation>, Error> {
        let binding = {
            let records = self.inner.records.read().map_err(|_| Error::Unavailable)?;
            let record = records.get(&run).ok_or(Error::NotFound)?;
            (
                record.request.workspace_id().into_bytes(),
                [
                    record.request.providers().writer().into_bytes(),
                    record.request.providers().reviewer().into_bytes(),
                    record.request.providers().fixer().into_bytes(),
                ],
                *record.interaction.workbench.conversation().as_bytes(),
                record.snapshot.task().to_owned(),
            )
        };
        let Some(reservation) =
            locked(&self.inner.improvements)?.reservation_for_run(run)?
        else {
            return Ok(None);
        };
        let route = &reservation.evaluation;
        let expected_title = format!("Harness improvement {}", store::hex(&reservation.id[..8]));
        if route.run != run.into_bytes()
            || route.target != binding.0
            || route.providers != binding.1
            || route.conversation != binding.2
            || binding.3 != expected_title
        {
            return Err(Error::internal(
                "bind improvement context sources",
                "the product run no longer matches its durable evaluation reservation",
            ));
        }
        Ok(Some(reservation))
    }
}

fn context_source(
    reservation: &store::Reservation,
    source: store::EvaluationSource,
) -> Result<ContextSource, Error> {
    let label = source.run().map_or_else(
        || format!("Selected improvement proposal {}", store::hex(&reservation.id)),
        |run| format!("Run observation {}", store::hex(run.as_bytes())),
    );
    ContextSource::new(
        source.ordinal(),
        label,
        source.body().digest().into_bytes(),
        source.body().bytes(),
    )
    .map_err(|error| Error::internal("project improvement context source", error))
}
