//! Digest-bound assembly of complete public conversation activity pages.

use peritus_app_protocol::{
    AppRequestPayload, ProductActivity, ProductActivityKind, ProductActivityPageQuery,
    ProductActivitySegment, ProductInteractionPage, ProductInteractionSnapshot,
    WellKnownProtocolFeature,
};
use peritus_types::{RunId, Sha256Digest};

use super::{AppModel, Effect, NoticeLevel, PendingRequest};

#[derive(Debug)]
pub(super) struct ActivityHistory {
    digest: Sha256Digest,
    expected_total: u64,
    activities: Vec<ProductActivity>,
    first_sequence: Option<u64>,
    partial: Option<PartialActivity>,
    complete: bool,
}

#[derive(Debug)]
struct PartialActivity {
    sequence: u64,
    kind: ProductActivityKind,
    next_segment: u64,
    text_bytes: u64,
    text: String,
    detail_bytes: u64,
    detail: String,
}

impl ActivityHistory {
    fn new(digest: Sha256Digest, expected_total: u64) -> Self {
        Self {
            digest,
            expected_total,
            activities: Vec::new(),
            first_sequence: None,
            partial: None,
            complete: false,
        }
    }

    fn is_complete(&self, digest: Sha256Digest, total: u64) -> bool {
        self.complete && self.digest == digest && self.expected_total == total
    }

    fn accept(&mut self, segments: &[ProductActivitySegment], final_page: bool) -> Result<(), ()> {
        if self.complete {
            return Err(());
        }
        for segment in segments {
            self.accept_segment(segment)?;
        }
        if final_page {
            self.finish_partial()?;
            if self.activities.last().map(ProductActivity::sequence).unwrap_or(0)
                != self.expected_total
            {
                return Err(());
            }
            self.complete = true;
        }
        Ok(())
    }

    fn accept_segment(&mut self, segment: &ProductActivitySegment) -> Result<(), ()> {
        if self
            .partial
            .as_ref()
            .is_some_and(|partial| partial.sequence != segment.sequence())
        {
            self.finish_partial()?;
        }
        if self.partial.is_none() {
            let expected_sequence = self.activities.last().map_or_else(
                || segment.sequence(),
                |activity| activity.sequence().checked_add(1).unwrap_or(0),
            );
            if segment.sequence() != expected_sequence
                || segment.segment() != 0
                || segment.text_offset() != 0
                || segment.detail_offset() != 0
            {
                return Err(());
            }
            if self.first_sequence.is_none() {
                self.first_sequence = Some(segment.sequence());
            }
            self.partial = Some(PartialActivity {
                sequence: segment.sequence(),
                kind: segment.kind(),
                next_segment: 0,
                text_bytes: segment.text_bytes(),
                text: String::new(),
                detail_bytes: segment.detail_bytes(),
                detail: String::new(),
            });
        }
        let partial = self.partial.as_mut().ok_or(())?;
        if segment.sequence() != partial.sequence
            || segment.kind() != partial.kind
            || segment.segment() != partial.next_segment
            || segment.text_bytes() != partial.text_bytes
            || segment.detail_bytes() != partial.detail_bytes
            || u64::try_from(partial.text.len()).map_err(|_| ())? != segment.text_offset()
            || u64::try_from(partial.detail.len()).map_err(|_| ())? != segment.detail_offset()
        {
            return Err(());
        }
        partial.text.push_str(segment.text());
        partial.detail.push_str(segment.detail());
        partial.next_segment = partial.next_segment.checked_add(1).ok_or(())?;
        if u64::try_from(partial.text.len()).map_err(|_| ())? > partial.text_bytes
            || u64::try_from(partial.detail.len()).map_err(|_| ())? > partial.detail_bytes
        {
            return Err(());
        }
        Ok(())
    }

    fn finish_partial(&mut self) -> Result<(), ()> {
        let Some(partial) = self.partial.take() else { return Ok(()) };
        if u64::try_from(partial.text.len()).map_err(|_| ())? != partial.text_bytes
            || u64::try_from(partial.detail.len()).map_err(|_| ())? != partial.detail_bytes
        {
            return Err(());
        }
        self.activities.push(
            ProductActivity::new(partial.sequence, partial.kind, partial.text, partial.detail)
                .map_err(|_| ())?,
        );
        Ok(())
    }
}

impl AppModel {
    pub(in crate::model) fn abandon_activity_pages(&mut self) {
        let requests = self
            .pending
            .iter()
            .filter_map(|(request, pending)| {
                matches!(pending, PendingRequest::ProductActivityPage(_)).then_some(*request)
            })
            .collect::<Vec<_>>();
        for request in requests {
            self.pending.remove(&request);
            self.pending_started.remove(&request);
        }
    }

    pub(in crate::model) fn supports_activity_pages(&self) -> bool {
        self.features.iter().any(|feature| {
            feature.as_str() == WellKnownProtocolFeature::ProductActivityPages.as_str()
        })
    }

    pub(super) fn request_activity_history(&mut self, run_id: RunId) -> Vec<Effect> {
        if !self.supports_activity_pages()
            || self.pending.values().any(|pending| {
                matches!(pending, PendingRequest::ProductActivityPage(query) if query.run_id() == run_id)
            })
        {
            return Vec::new();
        }
        let chat_run = self.chat.run_id;
        self.activity_histories
            .retain(|cached, _| *cached == run_id || Some(*cached) == chat_run);
        self.request_activity_page(ProductActivityPageQuery::first(run_id))
            .into_iter()
            .collect()
    }

    fn request_activity_page(&mut self, query: ProductActivityPageQuery) -> Option<Effect> {
        self.request(
            AppRequestPayload::QueryInteractionPage(query),
            PendingRequest::ProductActivityPage(query),
        )
    }

    pub(super) fn accept_activity_page(
        &mut self,
        page: &ProductInteractionPage,
        pending: Option<&PendingRequest>,
    ) -> Vec<Effect> {
        if !matches!(pending, Some(PendingRequest::ProductActivityPage(query)) if *query == page.query())
        {
            self.notice(NoticeLevel::Error, "Ignored a conversation history page for another request.");
            return Vec::new();
        }
        let interaction = page.interaction();
        let run_id = interaction.snapshot().run_id();
        let Some(window) = interaction.activity_window() else {
            self.notice(NoticeLevel::Error, "Conversation history page omitted its history binding.");
            return Vec::new();
        };
        self.accept_product_interaction(interaction.clone());
        if self.chat.run_id == Some(run_id) {
            self.accept_chat(interaction.clone());
        }

        if page.query().cursor().is_none()
            && self
                .activity_histories
                .get(&run_id)
                .is_some_and(|history| history.is_complete(window.history(), window.total()))
        {
            return Vec::new();
        }
        if page.query().cursor().is_none() {
            self.activity_histories
                .insert(run_id, ActivityHistory::new(window.history(), window.total()));
        }
        let accepted = self.activity_histories.get_mut(&run_id).is_some_and(|history| {
            history.digest == window.history()
                && history.expected_total == window.total()
                && history.accept(page.segments(), page.next().is_none()).is_ok()
        });
        if !accepted {
            self.activity_histories.remove(&run_id);
            self.notice(
                NoticeLevel::Error,
                "Conversation history page was discontinuous; retained the bounded live view.",
            );
            return Vec::new();
        }
        page.next()
            .and_then(|cursor| self.request_activity_page(ProductActivityPageQuery::after(cursor)))
            .into_iter()
            .collect()
    }

    pub(crate) fn interaction_activities<'a>(
        &'a self,
        snapshot: &'a ProductInteractionSnapshot,
    ) -> &'a [ProductActivity] {
        snapshot
            .activity_window()
            .and_then(|window| {
                self.activity_histories.get(&snapshot.snapshot().run_id()).filter(|history| {
                    history.is_complete(window.history(), window.total())
                })
            })
            .map_or_else(|| snapshot.activities(), |history| history.activities.as_slice())
    }

    pub(crate) fn interaction_history_is_complete(
        &self,
        snapshot: &ProductInteractionSnapshot,
    ) -> bool {
        let Some(window) = snapshot.activity_window() else { return false };
        self.activity_histories
            .get(&snapshot.snapshot().run_id())
            .is_some_and(|history| history.is_complete(window.history(), window.total()))
            || window.omitted() == 0
                && snapshot.activities().iter().all(|activity| {
                    u64::try_from(activity.text().len()).ok() == Some(activity.text_bytes())
                        && u64::try_from(activity.detail().len()).ok()
                            == Some(activity.detail_bytes())
                })
    }

    pub(crate) fn unavailable_activity_prefix(
        &self,
        snapshot: &ProductInteractionSnapshot,
    ) -> u64 {
        let Some(window) = snapshot.activity_window() else { return 0 };
        self.activity_histories
            .get(&snapshot.snapshot().run_id())
            .filter(|history| history.is_complete(window.history(), window.total()))
            .and_then(|history| history.first_sequence)
            .map_or(0, |sequence| sequence.saturating_sub(1))
    }
}
