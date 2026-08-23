use crate::support::{
    FixtureIds, PermissionSpec, PolicyInput, ScopeInput, action, command, descriptor, digest,
    grant, instant, permission, permission_selector, policy, registry, scope, use_limit, window,
};
use peritus_policy::{
    ActorRole, AuthorityTimeState, AuthorizationRequest, Capability, CapabilityUseRequest,
    OperationClass, PolicyErrorKind, ScopeDimension,
};

const SEEDS: [u64; 4] =
    [0x4341_5054_5241_4345, 0x5449_4d45_5241_4345, 0x5245_4a45_4354_4544, 0x4143_4345_5054_4544];
const ACTOR: u8 = 1;
const ROLE: u8 = 2;
const ENVIRONMENT: u8 = 4;
const REVISION: u8 = 8;
const PERMISSION: u8 = 16;

const fn permission_spec(ids: &FixtureIds) -> PermissionSpec {
    PermissionSpec { resource: ids.first_resource, name: "workspace.mutate" }
}

fn issue(ids: &FixtureIds, uses: Option<u64>) -> Capability {
    let permission = permission_spec(ids);
    let validity = window(1, 10, 100);
    let limit = use_limit(uses);
    let definition = policy(PolicyInput {
        actors: vec![ids.actor],
        roles: vec![ActorRole::Writer],
        environments: vec![ids.environment],
        permissions: vec![permission],
        revision: ids.revision(),
        validity,
        uses: limit,
        grants: vec![grant(
            10,
            permission_selector(ids.revision(), vec![permission]),
            validity,
            limit,
        )],
        immutable_denies: Vec::new(),
        operations: registry(vec![descriptor(
            "workspace.mutate",
            OperationClass::WorkspaceMutation,
        )]),
        layers: Vec::new(),
    });
    let requested = scope(ScopeInput {
        actor: ids.actor,
        role: ActorRole::Writer,
        environment: ids.environment,
        permissions: vec![permission],
        revision: ids.revision(),
        validity,
        uses: limit,
    });
    definition
        .evaluate(
            AuthorizationRequest::new(requested),
            AuthorityTimeState::new(instant(1, 0)),
            instant(1, 10),
        )
        .expect("oracle issuance evaluation")
        .into_parts()
        .0
        .expect("oracle issuance plan")
        .issue(command(1), digest(2))
        .into_capability()
}

#[derive(Clone, Copy, Debug)]
struct UseCommand {
    mismatches: u8,
    epoch: u64,
    tick: u64,
    action: u8,
}

impl UseCommand {
    const fn exact(tick: u64, action: u8) -> Self {
        Self { mismatches: 0, epoch: 1, tick, action }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ExpectedFailure {
    kind: PolicyErrorKind,
    dimension: Option<ScopeDimension>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CapabilityOracle {
    scope_limit: Option<u64>,
    remaining: Option<u64>,
    time_epoch: u64,
    greatest_tick: u64,
}

impl CapabilityOracle {
    const fn new(uses: Option<u64>) -> Self {
        Self { scope_limit: uses, remaining: uses, time_epoch: 1, greatest_tick: 10 }
    }

    fn apply(&mut self, command: UseCommand) -> Result<Option<u64>, ExpectedFailure> {
        let dimension = if command.mismatches & ACTOR != 0 {
            Some(ScopeDimension::Actor)
        } else if command.mismatches & ROLE != 0 {
            Some(ScopeDimension::Role)
        } else if command.mismatches & ENVIRONMENT != 0 {
            Some(ScopeDimension::Environment)
        } else if command.mismatches & REVISION != 0 {
            Some(ScopeDimension::Revision)
        } else if command.mismatches & PERMISSION != 0 {
            Some(ScopeDimension::Permissions)
        } else {
            None
        };
        if let Some(dimension) = dimension {
            return Err(ExpectedFailure {
                kind: PolicyErrorKind::CapabilityScopeMismatch,
                dimension: Some(dimension),
            });
        }
        let failure = if command.epoch != self.time_epoch {
            Some(PolicyErrorKind::ClockEpochMismatch)
        } else if command.tick < self.greatest_tick {
            Some(PolicyErrorKind::ClockRegression)
        } else if command.epoch != 1 {
            Some(PolicyErrorKind::ClockEpochMismatch)
        } else if command.tick < 10 {
            Some(PolicyErrorKind::CapabilityNotYetValid)
        } else if command.tick >= 100 {
            Some(PolicyErrorKind::CapabilityExpired)
        } else if self.remaining == Some(0) {
            Some(PolicyErrorKind::CapabilityExhausted)
        } else {
            None
        };
        if let Some(kind) = failure {
            return Err(ExpectedFailure { kind, dimension: None });
        }
        let previous = self.remaining;
        self.remaining = self.remaining.map(|value| value - 1);
        self.greatest_tick = command.tick;
        Ok(previous)
    }
}

fn request(ids: &FixtureIds, command: UseCommand) -> CapabilityUseRequest {
    let revision = if command.mismatches & REVISION == 0 {
        ids.revision()
    } else {
        peritus_types::RevisionTuple::new(
            ids.revision().acceptance_spec_id(),
            ids.revision().harness_id(),
            ids.revision().workspace_id(),
            ids.revision().workspace_generation(),
            ids.revision().workspace_revision(),
            ids.other_policy,
            ids.revision().provider_profile_id(),
        )
    };
    let permission_spec = if command.mismatches & PERMISSION == 0 {
        permission_spec(ids)
    } else {
        PermissionSpec { resource: ids.second_resource, name: "workspace.mutate" }
    };
    CapabilityUseRequest::new(
        action(command.action),
        digest(command.action.wrapping_add(40)),
        permission(permission_spec),
        if command.mismatches & ACTOR == 0 { ids.actor } else { ids.other_actor },
        if command.mismatches & ROLE == 0 { ActorRole::Writer } else { ActorRole::Reviewer },
        if command.mismatches & ENVIRONMENT == 0 { ids.environment } else { ids.other_environment },
        revision,
        instant(command.epoch, command.tick),
    )
}

#[derive(Clone, Copy, Debug)]
struct TraceAt {
    seed: u64,
    case: u64,
    step: usize,
    command: UseCommand,
}

fn assert_snapshot(actual: &Capability, expected: CapabilityOracle, ids: &FixtureIds, at: TraceAt) {
    let diagnostic = (at.seed, at.case, at.step, at.command);
    assert_eq!(actual.scope().actor_id(), ids.actor, "trace {diagnostic:?}");
    assert_eq!(actual.scope().role(), ActorRole::Writer, "trace {at:?}");
    assert_eq!(actual.scope().environment_id(), ids.environment, "trace {at:?}");
    assert_eq!(
        actual.scope().permissions().as_slice(),
        &[permission(permission_spec(ids))],
        "trace {at:?}"
    );
    assert_eq!(actual.scope().revision(), ids.revision(), "trace {at:?}");
    assert_eq!(actual.scope().validity(), window(1, 10, 100), "trace {at:?}");
    assert_eq!(actual.scope().use_limit().remaining(), expected.scope_limit, "trace {at:?}");
    assert_eq!(actual.remaining_uses().remaining(), expected.remaining, "trace {at:?}");
    assert_eq!(actual.issued_at(), instant(1, 10), "trace {at:?}");
    assert_eq!(actual.issuance_digest(), digest(2), "trace {at:?}");
    assert_eq!(actual.issuance_command_id(), command(1), "trace {at:?}");
    assert_eq!(actual.time_state().epoch().get(), expected.time_epoch, "trace {at:?}");
    assert_eq!(actual.time_state().greatest_tick_millis(), expected.greatest_tick, "trace {at:?}");
}

const fn scripted(step: usize) -> UseCommand {
    match step {
        0 => UseCommand { mismatches: 31, epoch: 2, tick: 0, action: 1 },
        1 => UseCommand { mismatches: 30, epoch: 2, tick: 0, action: 2 },
        2 => UseCommand { mismatches: 28, epoch: 2, tick: 0, action: 3 },
        3 => UseCommand { mismatches: 24, epoch: 2, tick: 0, action: 4 },
        4 => UseCommand { mismatches: 16, epoch: 2, tick: 0, action: 5 },
        5 => UseCommand { mismatches: 0, epoch: 2, tick: 0, action: 6 },
        6 => UseCommand::exact(9, 7),
        7 => UseCommand::exact(100, 8),
        8 => UseCommand::exact(20, 9),
        9 => UseCommand::exact(20, 10),
        10 => UseCommand::exact(30, 11),
        11 => UseCommand::exact(40, 12),
        12 => UseCommand::exact(100, 13),
        _ => UseCommand::exact(29, 14),
    }
}

struct Generator(u64);

impl Generator {
    const fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        self.0
    }

    const fn command(&mut self, step: usize) -> UseCommand {
        if step < 14 {
            return scripted(step);
        }
        let bits = self.next();
        let epoch = if bits & 0x20 == 0 { 1 } else { 2 };
        UseCommand {
            mismatches: bits.to_le_bytes()[0] & 0x1f,
            epoch,
            tick: (bits >> 8) % 111,
            action: step.to_le_bytes()[0].wrapping_add(bits.to_le_bytes()[3]) | 1,
        }
    }
}

#[test]
fn generated_capability_traces_match_full_independent_oracle_after_every_step() {
    for seed in SEEDS {
        for case in 0_u64..8 {
            let uses = if case % 2 == 0 { Some(3) } else { None };
            let ids = FixtureIds::new();
            let mut actual = issue(&ids, uses);
            let mut oracle = CapabilityOracle::new(uses);
            let mut generator = Generator(seed ^ case);
            for step in 0..48 {
                let command = generator.command(step);
                let before = oracle;
                let expected = oracle.apply(command);
                let at = TraceAt { seed, case, step, command };
                match (expected, actual.try_use(request(&ids, command), digest(90))) {
                    (Ok(previous), Ok(transition)) => {
                        assert_eq!(transition.action_id(), action(command.action), "trace {at:?}");
                        assert_eq!(
                            transition.action_digest(),
                            digest(command.action.wrapping_add(40)),
                            "trace {at:?}"
                        );
                        assert_eq!(
                            transition.permission(),
                            &permission(permission_spec(&ids)),
                            "trace {at:?}"
                        );
                        assert_eq!(
                            transition.used_at(),
                            instant(command.epoch, command.tick),
                            "trace {at:?}"
                        );
                        assert_eq!(transition.transition_digest(), digest(90), "trace {at:?}");
                        assert_eq!(
                            transition.previous_remaining().remaining(),
                            previous,
                            "trace {at:?}"
                        );
                        assert_snapshot(transition.successor(), oracle, &ids, at);
                        actual = transition.into_successor();
                    }
                    (Err(expected), Err(failure)) => {
                        assert_eq!(failure.error().kind(), expected.kind, "trace {at:?}");
                        assert_eq!(failure.error().dimension(), expected.dimension, "trace {at:?}");
                        assert_eq!(failure.error().collection(), None, "trace {at:?}");
                        assert_snapshot(failure.capability(), before, &ids, at);
                        actual = failure.into_capability();
                        oracle = before;
                    }
                    (expected, actual) => {
                        panic!("trace {at:?}: oracle {expected:?}, implementation {actual:?}")
                    }
                }
                assert_snapshot(&actual, oracle, &ids, at);
            }
        }
    }
}

#[test]
fn generated_authority_time_traces_match_independent_oracle_after_every_step() {
    for seed in SEEDS {
        let mut generator = Generator(seed);
        let mut actual = AuthorityTimeState::new(instant(1, 10));
        let epoch = 1;
        let mut greatest = 10;
        for step in 0..128 {
            let bits = generator.next();
            let candidate_epoch = if bits.trailing_zeros() >= 2 { 2 } else { 1 };
            let tick = (bits >> 8) % 160;
            let expected = if candidate_epoch != epoch {
                Err(PolicyErrorKind::ClockEpochMismatch)
            } else if tick < greatest {
                Err(PolicyErrorKind::ClockRegression)
            } else {
                greatest = tick;
                Ok(())
            };
            match (expected, actual.observe(instant(candidate_epoch, tick))) {
                (Ok(()), Ok(next)) => actual = next,
                (Err(kind), Err(failure)) => {
                    assert_eq!(failure.error().kind(), kind, "seed {seed:#018x} step {step}");
                    assert_eq!(
                        failure.state().epoch().get(),
                        epoch,
                        "seed {seed:#018x} step {step}"
                    );
                    assert_eq!(
                        failure.state().greatest_tick_millis(),
                        greatest,
                        "seed {seed:#018x} step {step}"
                    );
                    actual = failure.into_state();
                }
                (expected, actual) => {
                    panic!(
                        "seed {seed:#018x} step {step} epoch {candidate_epoch} tick {tick}: \
                         oracle {expected:?}, implementation {actual:?}"
                    )
                }
            }
            assert_eq!(actual.epoch().get(), epoch, "seed {seed:#018x} step {step}");
            assert_eq!(actual.greatest_tick_millis(), greatest, "seed {seed:#018x} step {step}");
        }
    }
}
