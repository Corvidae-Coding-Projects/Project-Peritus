//! Canonical bounded-goal definitions and projections.

use super::primitive::{invalid, read_id, unknown, write_id};
use crate::{
    MAX_WORKBENCH_GOAL_CRITERIA, WorkbenchGoalBudget, WorkbenchGoalCriterion,
    WorkbenchGoalCriterionDefinition, WorkbenchGoalCriterionKind as Kind,
    WorkbenchGoalCriterionState as CriterionState, WorkbenchGoalDefinition,
    WorkbenchGoalPauseMode as Pause, WorkbenchGoalRole as Role, WorkbenchGoalRoleUsage,
    WorkbenchGoalSnapshot, WorkbenchGoalState as State, WorkbenchGoalUsage,
};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};

pub(super) fn write_definition(
    w: &mut CanonicalWriter,
    value: &WorkbenchGoalDefinition,
) -> Result<(), CodecError> {
    w.write_str(value.objective().as_str())?;
    w.write_u16(
        u16::try_from(value.criteria().len())
            .map_err(|_| CodecError::at(CodecErrorKind::LimitExceeded, w.len()))?,
    )?;
    for criterion in value.criteria() {
        write_definition_criterion(w, criterion)?;
    }
    write_budget(w, value.budget())
}

pub(super) fn read_definition(
    r: &mut CanonicalReader<'_>,
) -> Result<WorkbenchGoalDefinition, CodecError> {
    let offset = r.offset();
    let objective = super::workbench_inputs::read_text(r)?;
    let count = usize::from(r.read_u16()?);
    if count > MAX_WORKBENCH_GOAL_CRITERIA {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    let mut criteria = Vec::with_capacity(count);
    for _ in 0..count {
        criteria.push(read_definition_criterion(r)?);
    }
    let budget = read_budget(r)?;
    invalid(offset, WorkbenchGoalDefinition::new(objective, criteria, budget))
}

pub(super) fn write_budget(
    w: &mut CanonicalWriter,
    value: WorkbenchGoalBudget,
) -> Result<(), CodecError> {
    write_option_u64(w, value.max_active_millis())?;
    write_option_u32(w, value.max_requests())?;
    write_option_u32(w, value.max_tool_calls())?;
    write_option_u64(w, value.max_total_tokens())
}

pub(super) fn read_budget(r: &mut CanonicalReader<'_>) -> Result<WorkbenchGoalBudget, CodecError> {
    let offset = r.offset();
    let active = read_option_u64(r)?;
    let requests = read_option_u32(r)?;
    let tools = read_option_u32(r)?;
    let tokens = read_option_u64(r)?;
    invalid(offset, WorkbenchGoalBudget::new(active, requests, tools, tokens))
}

pub(super) fn write_pause(w: &mut CanonicalWriter, value: Pause) -> Result<(), CodecError> {
    w.write_u16(match value {
        Pause::Now => 1,
        Pause::AfterOperation => 2,
        Pause::BeforeEdit => 3,
    })
}

pub(super) fn read_pause(r: &mut CanonicalReader<'_>) -> Result<Pause, CodecError> {
    let offset = r.offset();
    match r.read_u16()? {
        1 => Ok(Pause::Now),
        2 => Ok(Pause::AfterOperation),
        3 => Ok(Pause::BeforeEdit),
        _ => unknown(offset),
    }
}

pub(super) fn write_snapshot(
    w: &mut CanonicalWriter,
    value: &WorkbenchGoalSnapshot,
) -> Result<(), CodecError> {
    super::workbench::write_query(w, value.query())?;
    w.write_u64(value.aggregate_revision())?;
    write_id(w, value.goal().as_bytes())?;
    write_id(w, value.run().as_bytes())?;
    w.write_str(value.objective().as_str())?;
    write_state(w, value.state())?;
    w.write_str(value.reason())?;
    w.write_u64(value.user_revision())?;
    w.write_u32(value.attempt())?;
    w.write_bool(value.restart_eligible())?;
    w.write_option_tag(value.pause_mode().is_some())?;
    if let Some(mode) = value.pause_mode() {
        write_pause(w, mode)?;
    }
    w.write_u16(
        u16::try_from(value.criteria().len())
            .map_err(|_| CodecError::at(CodecErrorKind::LimitExceeded, w.len()))?,
    )?;
    for criterion in value.criteria() {
        write_criterion(w, criterion)?;
    }
    write_budget(w, value.budget())?;
    write_usage(w, value.usage())
}

pub(super) fn read_snapshot(
    r: &mut CanonicalReader<'_>,
) -> Result<WorkbenchGoalSnapshot, CodecError> {
    let offset = r.offset();
    let query = super::workbench::read_query(r)?;
    let aggregate_revision = r.read_u64()?;
    let goal = read_id(r, crate::ControlOperationId::new)?;
    let run = read_id(r, peritus_types::RunId::new)?;
    let objective = super::workbench_inputs::read_text(r)?;
    let state = read_state(r)?;
    let reason = r.read_str()?.to_owned();
    let user_revision = r.read_u64()?;
    let attempt = r.read_u32()?;
    let restart_eligible = r.read_bool()?;
    let pause_mode = if r.read_option_tag()? { Some(read_pause(r)?) } else { None };
    let count = usize::from(r.read_u16()?);
    if count > MAX_WORKBENCH_GOAL_CRITERIA {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    let mut criteria = Vec::with_capacity(count);
    for _ in 0..count {
        criteria.push(read_criterion(r)?);
    }
    let budget = read_budget(r)?;
    let usage = read_usage(r)?;
    invalid(
        offset,
        WorkbenchGoalSnapshot::new(
            query,
            aggregate_revision,
            goal,
            run,
            objective,
            state,
            reason,
            user_revision,
            attempt,
            restart_eligible,
            pause_mode,
            criteria,
            budget,
            usage,
        ),
    )
}

fn write_definition_criterion(
    w: &mut CanonicalWriter,
    value: &WorkbenchGoalCriterionDefinition,
) -> Result<(), CodecError> {
    write_kind(w, value.kind())?;
    w.write_str(value.description().as_str())?;
    w.write_bool(value.mandatory())
}

fn read_definition_criterion(
    r: &mut CanonicalReader<'_>,
) -> Result<WorkbenchGoalCriterionDefinition, CodecError> {
    Ok(WorkbenchGoalCriterionDefinition::new(
        read_kind(r)?,
        super::workbench_inputs::read_text(r)?,
        r.read_bool()?,
    ))
}

fn write_criterion(
    w: &mut CanonicalWriter,
    value: &WorkbenchGoalCriterion,
) -> Result<(), CodecError> {
    write_definition_criterion(w, value.definition())?;
    w.write_u16(match value.state() {
        CriterionState::Pending => 1,
        CriterionState::Satisfied => 2,
        CriterionState::Unavailable => 3,
        CriterionState::Stale => 4,
    })?;
    write_option_u64(w, value.evidence_revision())
}

fn read_criterion(r: &mut CanonicalReader<'_>) -> Result<WorkbenchGoalCriterion, CodecError> {
    let offset = r.offset();
    let definition = read_definition_criterion(r)?;
    let state = match r.read_u16()? {
        1 => CriterionState::Pending,
        2 => CriterionState::Satisfied,
        3 => CriterionState::Unavailable,
        4 => CriterionState::Stale,
        _ => return unknown(offset),
    };
    Ok(WorkbenchGoalCriterion::new(definition, state, read_option_u64(r)?))
}

fn write_kind(w: &mut CanonicalWriter, value: Kind) -> Result<(), CodecError> {
    w.write_u16(match value {
        Kind::RunnerAcceptance => 1,
        Kind::GraphicalPlaytest => 2,
        Kind::HumanValidation => 3,
    })
}

fn read_kind(r: &mut CanonicalReader<'_>) -> Result<Kind, CodecError> {
    let offset = r.offset();
    match r.read_u16()? {
        1 => Ok(Kind::RunnerAcceptance),
        2 => Ok(Kind::GraphicalPlaytest),
        3 => Ok(Kind::HumanValidation),
        _ => unknown(offset),
    }
}

pub(super) fn write_state(w: &mut CanonicalWriter, value: State) -> Result<(), CodecError> {
    w.write_u16(match value {
        State::Active => 1,
        State::WaitingForUser => 2,
        State::Pausing => 3,
        State::Paused => 4,
        State::Blocked => 5,
        State::BudgetReached => 6,
        State::Achieved => 7,
        State::Cancelled => 8,
    })
}

pub(super) fn read_state(r: &mut CanonicalReader<'_>) -> Result<State, CodecError> {
    let offset = r.offset();
    match r.read_u16()? {
        1 => Ok(State::Active),
        2 => Ok(State::WaitingForUser),
        3 => Ok(State::Pausing),
        4 => Ok(State::Paused),
        5 => Ok(State::Blocked),
        6 => Ok(State::BudgetReached),
        7 => Ok(State::Achieved),
        8 => Ok(State::Cancelled),
        _ => unknown(offset),
    }
}

fn write_usage(w: &mut CanonicalWriter, value: &WorkbenchGoalUsage) -> Result<(), CodecError> {
    for role in value.roles() {
        w.write_u16(match role.role() {
            Role::Writer => 1,
            Role::Reviewer => 2,
            Role::Fixer => 3,
        })?;
        w.write_u32(role.requests())?;
        w.write_u32(role.completed_requests())?;
        w.write_u32(role.tool_calls())?;
        write_option_u64(w, role.total_tokens())?;
        write_option_u64(w, role.provider_cost_microunits())?;
    }
    w.write_u64(value.active_millis())?;
    w.write_u64(value.wall_millis())?;
    w.write_u32(value.retries())?;
    w.write_u32(value.provider_failovers())?;
    w.write_u32(value.compactions())?;
    w.write_u64(value.workspace_bytes())?;
    w.write_u64(value.workspace_growth_bytes())?;
    w.write_u64(value.peak_rss_bytes())
}

fn read_usage(r: &mut CanonicalReader<'_>) -> Result<WorkbenchGoalUsage, CodecError> {
    let offset = r.offset();
    let mut rows = Vec::with_capacity(3);
    for expected in [Role::Writer, Role::Reviewer, Role::Fixer] {
        let role = match r.read_u16()? {
            1 => Role::Writer,
            2 => Role::Reviewer,
            3 => Role::Fixer,
            _ => return unknown(offset),
        };
        if role != expected {
            return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, offset));
        }
        rows.push(WorkbenchGoalRoleUsage::new(
            role,
            r.read_u32()?,
            r.read_u32()?,
            r.read_u32()?,
            read_option_u64(r)?,
            read_option_u64(r)?,
        ));
    }
    let roles: [WorkbenchGoalRoleUsage; 3] =
        rows.try_into().map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))?;
    Ok(WorkbenchGoalUsage::new(
        roles,
        r.read_u64()?,
        r.read_u64()?,
        r.read_u32()?,
        r.read_u32()?,
        r.read_u32()?,
        r.read_u64()?,
        r.read_u64()?,
        r.read_u64()?,
    ))
}

fn write_option_u64(w: &mut CanonicalWriter, value: Option<u64>) -> Result<(), CodecError> {
    w.write_option_tag(value.is_some())?;
    value.map_or(Ok(()), |value| w.write_u64(value))
}
fn read_option_u64(r: &mut CanonicalReader<'_>) -> Result<Option<u64>, CodecError> {
    if r.read_option_tag()? { Ok(Some(r.read_u64()?)) } else { Ok(None) }
}
fn write_option_u32(w: &mut CanonicalWriter, value: Option<u32>) -> Result<(), CodecError> {
    w.write_option_tag(value.is_some())?;
    value.map_or(Ok(()), |value| w.write_u32(value))
}
fn read_option_u32(r: &mut CanonicalReader<'_>) -> Result<Option<u32>, CodecError> {
    if r.read_option_tag()? { Ok(Some(r.read_u32()?)) } else { Ok(None) }
}
