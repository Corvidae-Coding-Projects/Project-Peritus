//! Additive conversation and discovery codecs; legacy payload bytes remain unchanged.

use super::{
    primitive::{invalid, read_id, write_id},
    product,
};
use crate::{
    MAX_PRODUCT_ACTIVITIES, MAX_PRODUCT_MODELS, ProductActivity, ProductActivityKind,
    ProductInteractionMode, ProductInteractionRequest, ProductInteractionSnapshot,
    ProductModelCatalog, ProductModelChoice, ProductModelEffort, ProductModelInfo,
    ProductModelQuery, ProductRoleModels,
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

pub(super) fn write_request(
    w: &mut CanonicalWriter,
    value: &ProductInteractionRequest,
) -> Result<(), CodecError> {
    product::write_run_request(w, value.request())?;
    w.write_u16(value.mode().tag())?;
    write_models(w, value.models())
}

pub(super) fn read_request(
    r: &mut CanonicalReader<'_>,
    efforts: bool,
) -> Result<ProductInteractionRequest, CodecError> {
    let request = product::read_run_request(r)?;
    let mode = read_mode(r)?;
    Ok(ProductInteractionRequest::new(request, mode, read_models(r, efforts)?))
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
        w.write_u64(activity.sequence())?;
        w.write_u16(activity.kind().tag())?;
        w.write_str(activity.text())?;
        w.write_str(activity.detail())?;
    }
    Ok(())
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
    let length = bounded_length(r, MAX_PRODUCT_ACTIVITIES)?;
    let mut activities = Vec::with_capacity(length);
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
    let length = bounded_length(r, MAX_PRODUCT_MODELS)?;
    let mut models = Vec::with_capacity(length);
    for _ in 0..length {
        let id = r.read_str()?.to_owned();
        let label = r.read_str()?.to_owned();
        let tools = if r.read_option_tag()? { Some(r.read_option_tag()?) } else { None };
        models.push(invalid(offset, ProductModelInfo::new(id, label, tools))?);
    }
    invalid(offset, ProductModelCatalog::new(profile, configured, models, timestamp, cached, error))
}

fn bounded_length(r: &mut CanonicalReader<'_>, limit: usize) -> Result<usize, CodecError> {
    let offset = r.offset();
    let length = r.read_collection_len()?;
    if length > limit {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    Ok(length)
}
