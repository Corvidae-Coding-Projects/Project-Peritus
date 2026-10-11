//! Bounded structured-diff page and raw-byte retrieval for review.

use peritus_app_protocol::{
    AppRequestPayload, WellKnownProtocolFeature, WorkbenchReviewDiffBytes,
    WorkbenchReviewDiffBytesQuery, WorkbenchReviewDiffPage, WorkbenchReviewDiffQuery,
    WorkbenchReviewPage, WorkbenchReviewQuery,
};

use super::ProductUi;
use crate::{
    action::Effect,
    model::{AppModel, NoticeLevel, PendingRequest},
};

impl AppModel {
    pub(in crate::model) fn accept_review_page(
        &mut self,
        requested: WorkbenchReviewQuery,
        page: WorkbenchReviewPage,
    ) -> Vec<Effect> {
        let pages_available = self.features.iter().any(|feature| {
            feature.as_str() == WellKnownProtocolFeature::WorkbenchReviewPages.as_str()
        });
        let selected = self
            .product
            .as_ref()
            .and_then(ProductUi::selected_run)
            .map(peritus_app_protocol::ProductRunSnapshot::run_id);
        let pending = self.product.as_ref().and_then(|product| product.review.pending);
        if page.query().query() != requested.query()
            || page.query().run() != requested.run()
            || page.query().offset() != requested.offset()
            || selected != Some(requested.run())
            || self.chat.workbench.selected != Some(requested.query())
            || pending.is_some_and(|pending| {
                pending.query() != requested.query()
                    || pending.run() != requested.run()
                    || pending.revision() != requested.revision()
                    || pending.offset() != requested.offset()
            })
        {
            self.notice(NoticeLevel::Error, "Mismatched structured-review response ignored.");
            return Vec::new();
        }
        if let Some(product) = &mut self.product {
            let review = &mut product.review;
            let prior_offset = review.page.as_ref().map(|prior| prior.query().offset());
            if review
                .page
                .as_ref()
                .is_none_or(|prior| prior.candidate_digest() != page.candidate_digest())
            {
                review.scroll = 0;
            }
            review.file = review.file.min(page.files().len().saturating_sub(1));
            review.hunk = review.hunk.min(
                page.files()
                    .get(review.file)
                    .map_or(0, |file| file.hunks().len().saturating_sub(1)),
            );
            review.comment = match prior_offset {
                Some(offset) if page.query().offset() > offset => 0,
                Some(offset) if page.query().offset() < offset => {
                    page.comments().len().saturating_sub(1)
                }
                _ => review.comment.min(page.comments().len().saturating_sub(1)),
            };
            review.message = format!(
                "Revision {} · {} comments · anchors are digest-bound",
                page.query().revision(),
                page.total_comments(),
            );
            review.page = Some(page);
            review.pending = None;
            review.diff_page = None;
            review.pending_diff = None;
            review.pending_raw = None;
            review.raw_line = None;
            review.raw_stream = false;
            review.raw_total_bytes = None;
            review.raw_lines.clear();
            review.diff_history.clear();
        }
        if !pages_available {
            return Vec::new();
        }
        let diff_request = WorkbenchReviewDiffQuery::new(
            requested.query(),
            requested.run(),
            self.product
                .as_ref()
                .and_then(|product| product.review.page.as_ref())
                .map_or_else(|| requested.revision(), |page| page.query().revision()),
            0,
            0,
            0,
        );
        self.request_review_diff(diff_request)
    }

    fn request_review_diff(&mut self, query: WorkbenchReviewDiffQuery) -> Vec<Effect> {
        if self.product.as_ref().and_then(|product| product.review.pending_diff) == Some(query) {
            return Vec::new();
        }
        let Some(effect) = self.request(
            AppRequestPayload::QueryWorkbenchReviewDiff(query),
            PendingRequest::WorkbenchReviewDiff(query),
        ) else {
            return Vec::new();
        };
        if let Some(product) = &mut self.product {
            product.review.pending_diff = Some(query);
            product.review.message = format!(
                "Loading diff page at file {} hunk {} line {}",
                query.file_offset(),
                query.hunk_offset(),
                query.line_offset()
            );
        }
        vec![effect]
    }

    pub(in crate::model) fn accept_review_diff_page(
        &mut self,
        requested: WorkbenchReviewDiffQuery,
        page: WorkbenchReviewDiffPage,
    ) -> Vec<Effect> {
        let Some(product) = &mut self.product else { return Vec::new() };
        let review = &mut product.review;
        if review.pending_diff != Some(requested) {
            return Vec::new();
        }
        if review.page.as_ref().is_none_or(|comments| {
            comments.query().query() != requested.query()
                || comments.query().run() != requested.run()
                || comments.query().revision() != requested.revision()
                || comments.candidate_digest() != page.candidate_digest()
                || comments.diff_digest() != page.diff_digest()
        }) || page.query() != requested
        {
            review.pending_diff = None;
            return Vec::new();
        }
        review.pending_diff = None;
        review.message = format!(
            "File {}/{} · hunk {} · {} lines total · digest-bound",
            page.file_anchor().path(),
            page.total_files(),
            requested.hunk_offset() + 1,
            page.total_lines(),
        );
        let raw_lines = page
            .lines()
            .iter()
            .filter(|line| line.is_truncated())
            .map(|line| (line.raw_offset(), line.raw_length()))
            .collect::<Vec<_>>();
        review.diff_page = Some(page);
        review.pending_raw = None;
        review.raw_line = None;
        review.raw_lines = raw_lines;
        review.raw_index = 0;
        if !review.raw_lines.is_empty() {
            let (offset, length) = review.raw_lines[0];
            let Some(page) = review.diff_page.as_ref() else { return Vec::new() };
            let raw_request = WorkbenchReviewDiffBytesQuery::new(
                requested.query(),
                requested.run(),
                requested.revision(),
                page.candidate_digest(),
                page.diff_digest(),
                offset,
                length.min(32 * 1024),
            );
            return self.request_review_diff_bytes(raw_request);
        }
        Vec::new()
    }

    fn request_review_diff_bytes(&mut self, query: WorkbenchReviewDiffBytesQuery) -> Vec<Effect> {
        if self.product.as_ref().and_then(|product| product.review.pending_raw) == Some(query) {
            return Vec::new();
        }
        let Some(effect) = self.request(
            AppRequestPayload::QueryWorkbenchReviewDiffBytes(query),
            PendingRequest::WorkbenchReviewDiffBytes(query),
        ) else {
            return Vec::new();
        };
        if let Some(product) = &mut self.product {
            product.review.pending_raw = Some(query);
        }
        vec![effect]
    }

    pub(in crate::model) fn request_raw_stream_location(&mut self, offset: u32) -> Vec<Effect> {
        let Some(review) = self.product.as_ref().map(|product| &product.review) else {
            return Vec::new();
        };
        let Some(page) = review.page.as_ref() else { return Vec::new() };
        let maximum = review
            .raw_total_bytes
            .map_or(32 * 1024, |total| total.saturating_sub(offset).min(32 * 1024));
        if maximum == 0 || offset > review.raw_total_bytes.unwrap_or(u32::MAX) {
            return Vec::new();
        }
        self.request_review_diff_bytes(WorkbenchReviewDiffBytesQuery::new(
            page.query().query(),
            page.query().run(),
            page.query().revision(),
            page.candidate_digest(),
            page.diff_digest(),
            offset,
            maximum,
        ))
    }

    pub(in crate::model) fn accept_review_diff_bytes(
        &mut self,
        requested: WorkbenchReviewDiffBytesQuery,
        bytes: &WorkbenchReviewDiffBytes,
    ) {
        let Some(product) = &mut self.product else { return };
        if product.review.raw_stream {
            let page = product.review.page.as_ref();
            if product.review.pending_raw != Some(requested)
                || bytes.query() != requested
                || page.is_none_or(|page| {
                    page.query().query() != requested.query()
                        || page.query().run() != requested.run()
                        || page.query().revision() != requested.revision()
                        || page.candidate_digest() != requested.candidate_digest()
                        || page.diff_digest() != requested.diff_digest()
                })
                || product.review.raw_total_bytes.is_some_and(|total| total != bytes.total_bytes())
            {
                return;
            }
            product.review.pending_raw = None;
            product.review.raw_total_bytes = Some(bytes.total_bytes());
            product.review.raw_line = Some((requested.offset(), bytes.bytes().to_vec()));
            product.review.message = format!(
                "Raw diff bytes {}..{} of {}",
                requested.offset(),
                requested
                    .offset()
                    .saturating_add(u32::try_from(bytes.bytes().len()).unwrap_or(u32::MAX)),
                bytes.total_bytes(),
            );
            return;
        }
        let page = product.review.diff_page.as_ref();
        if product.review.pending_raw != Some(requested)
            || bytes.query() != requested
            || page.is_none_or(|page| {
                page.query().query() != requested.query()
                    || page.query().run() != requested.run()
                    || page.query().revision() != requested.revision()
                    || page.candidate_digest() != requested.candidate_digest()
                    || page.diff_digest() != requested.diff_digest()
            })
        {
            return;
        }
        product.review.pending_raw = None;
        product.review.raw_line = Some((requested.offset(), bytes.bytes().to_vec()));
        product.review.message = format!(
            "Exact source bytes {}..{} of {} loaded",
            requested.offset(),
            requested
                .offset()
                .saturating_add(u32::try_from(bytes.bytes().len()).unwrap_or(u32::MAX),),
            bytes.total_bytes()
        );
    }

    fn request_raw_location(&mut self, index: usize, offset: u32) -> Vec<Effect> {
        let Some(review) = self.product.as_ref().map(|product| &product.review) else {
            return Vec::new();
        };
        if review.pending_raw.is_some() {
            return Vec::new();
        }
        let Some(page) = review.diff_page.as_ref() else { return Vec::new() };
        let Some((line_start, line_length)) = review.raw_lines.get(index).copied() else {
            return Vec::new();
        };
        let line_end = line_start.saturating_add(line_length);
        if offset < line_start || offset >= line_end {
            return Vec::new();
        }
        let length = line_end.saturating_sub(offset).min(32 * 1024);
        let query = WorkbenchReviewDiffBytesQuery::new(
            page.query().query(),
            page.query().run(),
            page.query().revision(),
            page.candidate_digest(),
            page.diff_digest(),
            offset,
            length,
        );
        let effects = self.request_review_diff_bytes(query);
        if !effects.is_empty()
            && let Some(product) = &mut self.product
        {
            product.review.raw_index = index;
            product.review.raw_line = None;
        }
        effects
    }

    pub(super) fn move_raw_cursor(&mut self, forward: bool) -> Vec<Effect> {
        let Some(review) = self.product.as_ref().map(|product| &product.review) else {
            return Vec::new();
        };
        if review.pending_raw.is_some() {
            return Vec::new();
        }
        if review.raw_stream {
            let Some((chunk_offset, chunk)) = review.raw_line.as_ref() else {
                return self.request_raw_stream_location(0);
            };
            let total = review.raw_total_bytes.unwrap_or(0);
            let next = chunk_offset.saturating_add(u32::try_from(chunk.len()).unwrap_or(u32::MAX));
            let target = if forward {
                (next < total).then_some(next)
            } else if *chunk_offset > 0 {
                Some(chunk_offset.saturating_sub(32 * 1024))
            } else {
                None
            };
            return target.map_or_else(Vec::new, |offset| self.request_raw_stream_location(offset));
        }
        let Some((chunk_offset, chunk)) = review.raw_line.as_ref() else { return Vec::new() };
        let index = review.raw_index;
        let Some((line_start, line_length)) = review.raw_lines.get(index).copied() else {
            return Vec::new();
        };
        let line_end = line_start.saturating_add(line_length);
        if forward {
            let next = chunk_offset.saturating_add(u32::try_from(chunk.len()).unwrap_or(u32::MAX));
            if next < line_end {
                return self.request_raw_location(index, next);
            }
            if index + 1 < review.raw_lines.len() {
                return self.request_raw_location(index + 1, review.raw_lines[index + 1].0);
            }
        } else if *chunk_offset > line_start {
            return self.request_raw_location(
                index,
                (*chunk_offset).saturating_sub(32 * 1024).max(line_start),
            );
        } else if index > 0 {
            let (prior_start, prior_length) = review.raw_lines[index - 1];
            return self.request_raw_location(
                index - 1,
                prior_start.saturating_add(prior_length).saturating_sub(32 * 1024).max(prior_start),
            );
        }
        Vec::new()
    }

    pub(super) fn next_review_diff_page(&mut self) -> Vec<Effect> {
        let Some(page) =
            self.product.as_ref().and_then(|product| product.review.diff_page.as_ref())
        else {
            return Vec::new();
        };
        if self.product.as_ref().is_some_and(|product| product.review.pending_diff.is_some()) {
            return Vec::new();
        }
        let (cursor, next) = (page.query(), page.next());
        let Some(next) = next else { return Vec::new() };
        let effects = self.request_review_diff(next);
        if !effects.is_empty()
            && let Some(product) = &mut self.product
        {
            product.review.diff_history.push(cursor);
        }
        effects
    }

    pub(super) fn previous_review_diff_page(&mut self) -> Vec<Effect> {
        if self.product.as_ref().is_some_and(|product| product.review.pending_diff.is_some()) {
            return Vec::new();
        }
        let Some(previous) =
            self.product.as_ref().and_then(|product| product.review.diff_history.last().copied())
        else {
            return Vec::new();
        };
        let effects = self.request_review_diff(previous);
        if !effects.is_empty()
            && let Some(product) = &mut self.product
        {
            product.review.diff_history.pop();
        }
        effects
    }
}
