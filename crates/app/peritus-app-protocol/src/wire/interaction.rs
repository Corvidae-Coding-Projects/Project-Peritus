//! Interaction and model-discovery codecs.

use super::{
    primitive::{invalid, read_digest, read_id, write_digest, write_id},
    product,
};
use crate::{
    MAX_PRODUCT_ACTIVITIES, MAX_PRODUCT_ACTIVITY_BYTES, MAX_PRODUCT_ACTIVITY_PAGE_SEGMENTS,
    MAX_PRODUCT_MODELS, MAX_PRODUCT_RETAINED_ERRORS, ProductActivity, ProductActivityKind,
    ProductActivityPageCursor, ProductActivityPageQuery, ProductActivitySegment,
    ProductActivityWindow, ProductInteractionMode, ProductInteractionPage,
    ProductInteractionSnapshot, ProductModelCatalog, ProductModelChoice, ProductModelEffort,
    ProductModelInfo, ProductModelQuery, ProductRoleModels,
};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_types::ProviderProfileId;

pub(super) fn write_model_update(
    w: &mut CanonicalWriter,
    value: &crate::ProductModelUpdate,
) -> Result<(), CodecError> {
    write_id(w, value.run_id().as_bytes())?;
    write_models(w, value.models())
}

pub(super) fn read_model_update(
    r: &mut CanonicalReader<'_>,
    efforts: bool,
) -> Result<crate::ProductModelUpdate, CodecError> {
    Ok(crate::ProductModelUpdate::new(
        read_id(r, peritus_types::RunId::new)?,
        read_models(r, efforts)?,
    ))
}

fn read_mode(r: &mut CanonicalReader<'_>) -> Result<ProductInteractionMode, CodecError> {
    let offset = r.offset();
    ProductInteractionMode::from_tag(r.read_u16()?)
        .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, offset))
}

pub(super) fn write_models(
    w: &mut CanonicalWriter,
    models: &ProductRoleModels,
) -> Result<(), CodecError> {
    for choice in [models.writer(), models.reviewer(), models.fixer()] {
        w.write_str(choice.id())?;
        w.write_option_tag(choice.manual())?;
        if models.has_effort() {
            w.write_u16(choice.effort().tag())?;
        }
    }
    Ok(())
}

pub(super) fn read_choice(
    r: &mut CanonicalReader<'_>,
    efforts: bool,
) -> Result<ProductModelChoice, CodecError> {
    let offset = r.offset();
    let id = r.read_str()?.to_owned();
    let manual = r.read_option_tag()?;
    let effort = if efforts {
        let offset = r.offset();
        ProductModelEffort::from_tag(r.read_u16()?)
            .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, offset))?
    } else {
        ProductModelEffort::Default
    };
    let choice = if id.is_empty() && !manual {
        ProductModelChoice::default()
    } else {
        invalid(offset, ProductModelChoice::new(id, manual))?
    };
    Ok(choice.with_effort(effort))
}

pub(super) fn read_models(
    r: &mut CanonicalReader<'_>,
    efforts: bool,
) -> Result<ProductRoleModels, CodecError> {
    let models = ProductRoleModels::new(
        read_choice(r, efforts)?,
        read_choice(r, efforts)?,
        read_choice(r, efforts)?,
    );
    if efforts && !models.has_effort() {
        return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, r.offset()));
    }
    Ok(models)
}

pub(super) fn write_model_query(
    w: &mut CanonicalWriter,
    query: ProductModelQuery,
) -> Result<(), CodecError> {
    write_id(w, query.profile().as_bytes())?;
    w.write_option_tag(query.refresh())
}

pub(super) fn read_model_query(
    r: &mut CanonicalReader<'_>,
) -> Result<ProductModelQuery, CodecError> {
    Ok(ProductModelQuery::new(read_id(r, ProviderProfileId::new)?, r.read_option_tag()?))
}

pub(super) fn write_snapshot(
    w: &mut CanonicalWriter,
    value: &ProductInteractionSnapshot,
) -> Result<(), CodecError> {
    w.write_option_tag(value.settlement().is_some())?;
    if let Some(settlement) = value.settlement() {
        let settled = invalid(
            w.len(),
            crate::ProductRunSettlementSnapshot::new(value.snapshot().clone(), *settlement),
        )?;
        product::write_settlement_snapshot(w, &settled)?;
    } else {
        product::write_snapshot(w, value.snapshot())?;
    }
    w.write_u16(value.mode().tag())?;
    write_models(w, value.models())?;
    w.write_u64(value.received())?;
    w.write_u64(value.incorporated())?;
    w.write_collection_len(value.activities().len())?;
    for activity in value.activities() {
        write_legacy_activity(w, activity)?;
    }
    Ok(())
}

fn write_legacy_activity(
    w: &mut CanonicalWriter,
    activity: &ProductActivity,
) -> Result<(), CodecError> {
    w.write_u64(activity.sequence())?;
    w.write_u16(activity.kind().tag())?;
    w.write_str(&legacy_field(activity.text(), activity.text_bytes()))?;
    w.write_str(&legacy_field(activity.detail(), activity.detail_bytes()))
}

fn legacy_field(value: &str, total: u64) -> String {
    if usize::try_from(total).ok() == Some(value.len()) {
        return value.to_owned();
    }
    let suffix = format!("\n[… {} total UTF-8 bytes; complete history requires activity paging]", total);
    let mut end = value.len().min(MAX_PRODUCT_ACTIVITY_BYTES.saturating_sub(suffix.len()));
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{suffix}", &value[..end])
}

fn write_activity(
    w: &mut CanonicalWriter,
    activity: &ProductActivity,
) -> Result<(), CodecError> {
    w.write_u64(activity.sequence())?;
    w.write_u16(activity.kind().tag())?;
    w.write_str(activity.text())?;
    w.write_u64(activity.text_bytes())?;
    w.write_str(activity.detail())?;
    w.write_u64(activity.detail_bytes())
}

fn read_activity(r: &mut CanonicalReader<'_>) -> Result<ProductActivity, CodecError> {
    let offset = r.offset();
    let sequence = r.read_u64()?;
    let tag_offset = r.offset();
    let kind = ProductActivityKind::from_tag(r.read_u16()?)
        .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, tag_offset))?;
    let text = r.read_str()?.to_owned();
    let text_bytes = r.read_u64()?;
    let detail = r.read_str()?.to_owned();
    let detail_bytes = r.read_u64()?;
    invalid(
        offset,
        ProductActivity::preview(sequence, kind, text, text_bytes, detail, detail_bytes),
    )
}

fn write_snapshot_with_window(
    w: &mut CanonicalWriter,
    value: &ProductInteractionSnapshot,
) -> Result<(), CodecError> {
    w.write_option_tag(value.settlement().is_some())?;
    if let Some(settlement) = value.settlement() {
        let settled = invalid(
            w.len(),
            crate::ProductRunSettlementSnapshot::new(value.snapshot().clone(), *settlement),
        )?;
        product::write_settlement_snapshot(w, &settled)?;
    } else {
        product::write_snapshot(w, value.snapshot())?;
    }
    w.write_u16(value.mode().tag())?;
    write_models(w, value.models())?;
    w.write_u64(value.received())?;
    w.write_u64(value.incorporated())?;
    w.write_collection_len(value.activities().len())?;
    for activity in value.activities() {
        write_activity(w, activity)?;
    }
    let window = value.activity_window().ok_or_else(|| {
        CodecError::at(CodecErrorKind::InvalidDomainValue, w.len())
    })?;
    write_digest(w, window.history())?;
    w.write_u64(window.total())?;
    w.write_u64(window.omitted())?;
    w.write_u64(window.omitted_errors())?;
    w.write_collection_len(window.retained_errors().len())?;
    for activity in window.retained_errors() {
        write_activity(w, activity)?;
    }
    w.write_option_tag(window.terminal_error().is_some())?;
    if let Some(activity) = window.terminal_error() {
        write_activity(w, activity)?;
    }
    Ok(())
}

fn read_snapshot_with_window(
    r: &mut CanonicalReader<'_>,
) -> Result<ProductInteractionSnapshot, CodecError> {
    let offset = r.offset();
    let (snapshot, settlement) = if r.read_option_tag()? {
        let settled = product::read_settlement_snapshot(r)?;
        (settled.snapshot().clone(), Some(*settled.settlement()))
    } else {
        (product::read_snapshot(r)?, None)
    };
    let mode = read_mode(r)?;
    let models = read_models(r, true)?;
    let received = r.read_u64()?;
    let incorporated = r.read_u64()?;
    let length = bounded_length(r, MAX_PRODUCT_ACTIVITIES, 8 + 2 + 4 + 8 + 4 + 8)?;
    let mut activities = r.reserve_collection(length)?;
    for _ in 0..length {
        activities.push(read_activity(r)?);
    }
    let history = read_digest(r)?;
    let total = r.read_u64()?;
    let omitted = r.read_u64()?;
    let omitted_errors = r.read_u64()?;
    let retained_length = bounded_length(r, MAX_PRODUCT_RETAINED_ERRORS, 8 + 2 + 4 + 8 + 4 + 8)?;
    let mut retained_errors = r.reserve_collection(retained_length)?;
    for _ in 0..retained_length {
        retained_errors.push(read_activity(r)?);
    }
    let terminal_error = if r.read_option_tag()? { Some(read_activity(r)?) } else { None };
    let window = invalid(
        offset,
        ProductActivityWindow::new(
            history,
            total,
            omitted,
            retained_errors,
            omitted_errors,
            terminal_error,
        ),
    )?;
    invalid(
        offset,
        ProductInteractionSnapshot::new(
            snapshot,
            mode,
            models,
            received,
            incorporated,
            activities,
            settlement,
        )
        .and_then(|snapshot| snapshot.with_activity_window(window)),
    )
}

pub(super) fn write_page_query(
    w: &mut CanonicalWriter,
    value: ProductActivityPageQuery,
) -> Result<(), CodecError> {
    write_id(w, value.run_id().as_bytes())?;
    w.write_option_tag(value.cursor().is_some())?;
    if let Some(cursor) = value.cursor() {
        write_cursor(w, cursor)?;
    }
    Ok(())
}

pub(super) fn read_page_query(
    r: &mut CanonicalReader<'_>,
) -> Result<ProductActivityPageQuery, CodecError> {
    let offset = r.offset();
    let run_id = read_id(r, peritus_types::RunId::new)?;
    if r.read_option_tag()? {
        let cursor = read_cursor(r)?;
        if cursor.run_id() != run_id {
            return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, offset));
        }
        Ok(ProductActivityPageQuery::after(cursor))
    } else {
        Ok(ProductActivityPageQuery::first(run_id))
    }
}

fn write_cursor(
    w: &mut CanonicalWriter,
    value: ProductActivityPageCursor,
) -> Result<(), CodecError> {
    write_id(w, value.run_id().as_bytes())?;
    write_digest(w, value.history())?;
    w.write_u64(value.after_sequence())?;
    w.write_u64(value.after_segment())
}

fn read_cursor(r: &mut CanonicalReader<'_>) -> Result<ProductActivityPageCursor, CodecError> {
    let offset = r.offset();
    invalid(
        offset,
        ProductActivityPageCursor::new(
            read_id(r, peritus_types::RunId::new)?,
            read_digest(r)?,
            r.read_u64()?,
            r.read_u64()?,
        ),
    )
}

pub(super) fn write_page(
    w: &mut CanonicalWriter,
    value: &ProductInteractionPage,
) -> Result<(), CodecError> {
    write_page_query(w, value.query())?;
    write_snapshot_with_window(w, value.interaction())?;
    w.write_collection_len(value.segments().len())?;
    for segment in value.segments() {
        w.write_u64(segment.sequence())?;
        w.write_u64(segment.segment())?;
        w.write_u16(segment.kind().tag())?;
        w.write_u64(segment.text_offset())?;
        w.write_str(segment.text())?;
        w.write_u64(segment.text_bytes())?;
        w.write_u64(segment.detail_offset())?;
        w.write_str(segment.detail())?;
        w.write_u64(segment.detail_bytes())?;
    }
    w.write_option_tag(value.next().is_some())?;
    if let Some(cursor) = value.next() {
        write_cursor(w, cursor)?;
    }
    Ok(())
}

pub(super) fn read_page(r: &mut CanonicalReader<'_>) -> Result<ProductInteractionPage, CodecError> {
    let offset = r.offset();
    let query = read_page_query(r)?;
    let interaction = read_snapshot_with_window(r)?;
    let length = bounded_length(
        r,
        MAX_PRODUCT_ACTIVITY_PAGE_SEGMENTS,
        8 + 8 + 2 + 8 + 4 + 8 + 8 + 4 + 8,
    )?;
    let mut segments = r.reserve_collection(length)?;
    for _ in 0..length {
        let sequence = r.read_u64()?;
        let segment = r.read_u64()?;
        let tag_offset = r.offset();
        let kind = ProductActivityKind::from_tag(r.read_u16()?)
            .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, tag_offset))?;
        segments.push(invalid(
            offset,
            ProductActivitySegment::new(
                sequence,
                segment,
                kind,
                r.read_u64()?,
                r.read_str()?.to_owned(),
                r.read_u64()?,
                r.read_u64()?,
                r.read_str()?.to_owned(),
                r.read_u64()?,
            ),
        )?);
    }
    let next = if r.read_option_tag()? { Some(read_cursor(r)?) } else { None };
    invalid(offset, ProductInteractionPage::new(query, interaction, segments, next))
}

pub(super) fn write_binding(
    writer: &mut CanonicalWriter,
    binding: &crate::ProductInteractionBinding,
) -> Result<(), CodecError> {
    writer.write_option_tag(binding.interaction().models().has_effort())?;
    write_snapshot(writer, binding.interaction())?;
    super::workbench::write_query(writer, binding.conversation())
}

pub(super) fn read_binding(
    reader: &mut CanonicalReader<'_>,
) -> Result<crate::ProductInteractionBinding, CodecError> {
    let offset = reader.offset();
    let efforts = reader.read_option_tag()?;
    let interaction = read_snapshot(reader, efforts)?;
    let conversation = super::workbench::read_query(reader)?;
    invalid(offset, crate::ProductInteractionBinding::new(interaction, conversation))
}

pub(super) fn read_snapshot(
    r: &mut CanonicalReader<'_>,
    efforts: bool,
) -> Result<ProductInteractionSnapshot, CodecError> {
    let offset = r.offset();
    let (snapshot, settlement) = if r.read_option_tag()? {
        let settled = product::read_settlement_snapshot(r)?;
        (settled.snapshot().clone(), Some(*settled.settlement()))
    } else {
        (product::read_snapshot(r)?, None)
    };
    let mode = read_mode(r)?;
    let models = read_models(r, efforts)?;
    let received = r.read_u64()?;
    let incorporated = r.read_u64()?;
    let length = bounded_length(r, MAX_PRODUCT_ACTIVITIES, 8 + 2 + 4 + 4)?;
    let mut activities = r.reserve_collection(length)?;
    for _ in 0..length {
        let sequence = r.read_u64()?;
        let tag_offset = r.offset();
        let kind = ProductActivityKind::from_tag(r.read_u16()?)
            .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, tag_offset))?;
        activities.push(invalid(
            offset,
            ProductActivity::new(
                sequence,
                kind,
                r.read_str()?.to_owned(),
                r.read_str()?.to_owned(),
            ),
        )?);
    }
    invalid(
        offset,
        ProductInteractionSnapshot::new(
            snapshot,
            mode,
            models,
            received,
            incorporated,
            activities,
            settlement,
        ),
    )
}

pub(super) fn write_catalog(
    w: &mut CanonicalWriter,
    value: &ProductModelCatalog,
) -> Result<(), CodecError> {
    write_id(w, value.profile().as_bytes())?;
    w.write_str(value.configured())?;
    w.write_u64(value.fetched_unix_seconds())?;
    w.write_option_tag(value.cached())?;
    w.write_str(value.error())?;
    w.write_collection_len(value.models().len())?;
    for model in value.models() {
        w.write_str(model.id())?;
        w.write_str(model.label())?;
        w.write_option_tag(model.tools().is_some())?;
        if let Some(tools) = model.tools() {
            w.write_option_tag(tools)?;
        }
    }
    Ok(())
}

pub(super) fn read_catalog(r: &mut CanonicalReader<'_>) -> Result<ProductModelCatalog, CodecError> {
    let offset = r.offset();
    let profile = read_id(r, ProviderProfileId::new)?;
    let configured = r.read_str()?.to_owned();
    let timestamp = r.read_u64()?;
    let cached = r.read_option_tag()?;
    let error = r.read_str()?.to_owned();
    let length = bounded_length(r, MAX_PRODUCT_MODELS, 4 + 4 + 1)?;
    let mut models = r.reserve_collection(length)?;
    for _ in 0..length {
        let id = r.read_str()?.to_owned();
        let label = r.read_str()?.to_owned();
        let tools = if r.read_option_tag()? { Some(r.read_option_tag()?) } else { None };
        models.push(invalid(offset, ProductModelInfo::new(id, label, tools))?);
    }
    invalid(offset, ProductModelCatalog::new(profile, configured, models, timestamp, cached, error))
}

fn bounded_length(
    r: &mut CanonicalReader<'_>,
    limit: usize,
    minimum_item_bytes: usize,
) -> Result<usize, CodecError> {
    let offset = r.offset();
    let length = r.read_collection_len(minimum_item_bytes)?;
    if length > limit {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    Ok(length)
}
