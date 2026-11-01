//! Explicit version intervals and real disposable SQLite migration recovery.

use peritus_migrations::{
    ApplicationCompatibility, BackupPolicy, MigrationConfig, MigrationDescriptor, MigrationEngine,
    MigrationErrorCode, MigrationFailpoint, MigrationHooks, MigrationOperationId, MigrationPlan,
    MigrationRegistry, MigrationVersion, RecoveryAction, RecoveryState,
};
use proptest::prelude::*;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::Path, sync::OnceLock, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cut {
    None,
    AfterRecord,
    BeforeBackup,
    AfterBackup,
    BeforeStep(u8),
    BeforeCommit,
    AfterCommit,
    SqlError,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "family", rename_all = "snake_case")]
pub enum Case {
    Plan {
        current: u8,
        target: u8,
        minimum: u8,
        maximum: u8,
        database: u64,
        available: u64,
        reserve: u64,
    },
    Execute {
        current: u8,
        target: u8,
        backup: bool,
        cut: Cut,
        corrupt_backup: bool,
        payload: u8,
        reopens: u8,
    },
    CurrentInstall {
        reopens: u8,
    },
}
pub fn strategy(_max_steps: usize) -> BoxedStrategy<Case> {
    let amount = prop_oneof![
        Just(0),
        Just(1),
        Just(65_536),
        Just(4_194_304),
        Just(u64::MAX),
        any::<u32>().prop_map(u64::from)
    ];
    let plan = (0_u8..=6, 1_u8..=6, 0_u8..=6, 1_u8..=6, amount.clone(), amount.clone(), amount)
        .prop_map(|(current, target, minimum, maximum, database, available, reserve)| Case::Plan {
            current,
            target,
            minimum,
            maximum,
            database,
            available,
            reserve,
        });
    let intervals: Vec<(u8, u8)> =
        (1_u8..=4).flat_map(|target| (0..=target).map(move |current| (current, target))).collect();
    let boundary =
        (proptest::sample::select(intervals), any::<u32>(), 0_u16..=127, 0_u8..=3, any::<bool>())
            .prop_map(|((mut current, target), small_database, small_reserve, class, tight)| {
                // Overflow needs a selected step; no-op capacity has only one u64 reserve addend.
                if class == 3 && current == target {
                    current = target - 1;
                }
                let scratch: u64 = (u64::from(current) + 1..=u64::from(target))
                    .map(|version| if version == 1 { 65_536 } else { 1_048_576 })
                    .sum();
                let backup = current < target;
                let mut database = u64::from(small_database);
                let mut reserve = u64::from(small_reserve);
                let available = match class {
                    0 => reserve + scratch + if backup { database } else { 0 },
                    1 => {
                        if !backup && reserve == 0 {
                            reserve = 1;
                        }
                        reserve + scratch + if backup { database } else { 0 } - 1
                    }
                    2 => {
                        if backup {
                            database = u64::MAX - scratch - reserve;
                        } else {
                            database = u64::MAX;
                            reserve = u64::MAX;
                        }
                        u64::MAX
                    }
                    _ => {
                        database = u64::MAX - scratch - reserve + 1;
                        u64::MAX
                    }
                };
                Case::Plan {
                    current,
                    target,
                    minimum: if tight { current } else { 0 },
                    maximum: if tight { target } else { 4 },
                    database,
                    available,
                    reserve,
                }
            });
    let execute = (
        0_u8..=2,
        1_u8..=3,
        any::<bool>(),
        prop_oneof![
            Just(Cut::None),
            Just(Cut::AfterRecord),
            Just(Cut::BeforeBackup),
            Just(Cut::AfterBackup),
            (0_u8..=2).prop_map(Cut::BeforeStep),
            Just(Cut::BeforeCommit),
            Just(Cut::AfterCommit),
            Just(Cut::SqlError)
        ],
        any::<bool>(),
        any::<u8>(),
        1_u8..=3,
    )
        .prop_map(|(from, to, mut backup, cut, corrupt, payload, reopens)| {
            if matches!(cut, Cut::BeforeBackup | Cut::AfterBackup | Cut::SqlError) {
                backup = true;
            }
            let target = if cut == Cut::SqlError { 3 } else { to };
            let current = from % target;
            Case::Execute {
                current,
                target,
                backup,
                cut,
                corrupt_backup: corrupt
                    && backup
                    && matches!(cut, Cut::AfterBackup | Cut::BeforeCommit),
                payload,
                reopens,
            }
        });
    prop_oneof![2=>plan,3=>boundary,6=>execute,1=>(1_u8..=3).prop_map(|reopens|Case::CurrentInstall{reopens})]
        .boxed()
}
pub fn calibration() -> Case {
    Case::Execute {
        current: 0,
        target: 2,
        backup: true,
        cut: Cut::AfterCommit,
        corrupt_backup: false,
        payload: 1,
        reopens: 1,
    }
}
pub fn examples() -> Vec<Case> {
    let mut cases = vec![
        calibration(),
        Case::CurrentInstall { reopens: 2 },
        Case::Plan {
            current: 0,
            target: 1,
            minimum: 0,
            maximum: 4,
            database: 1,
            available: 65_537,
            reserve: 0,
        },
        Case::Plan {
            current: 0,
            target: 4,
            minimum: 0,
            maximum: 4,
            database: u64::MAX,
            available: u64::MAX,
            reserve: 1,
        },
        Case::Plan {
            current: 4,
            target: 4,
            minimum: 0,
            maximum: 4,
            database: u64::MAX,
            available: 0,
            reserve: 0,
        },
        Case::Plan {
            current: 4,
            target: 3,
            minimum: 0,
            maximum: 4,
            database: 1,
            available: u64::MAX,
            reserve: 0,
        },
    ];
    for backup in [false, true] {
        for cut in [
            Cut::None,
            Cut::AfterRecord,
            Cut::BeforeStep(0),
            Cut::BeforeCommit,
            Cut::AfterCommit,
            Cut::SqlError,
        ] {
            if cut == Cut::SqlError && !backup {
                continue;
            }
            cases.push(Case::Execute {
                current: 1,
                target: 3,
                backup,
                cut,
                corrupt_backup: false,
                payload: 99,
                reopens: 2,
            });
        }
    }
    for cut in [Cut::BeforeBackup, Cut::AfterBackup, Cut::BeforeCommit] {
        cases.push(Case::Execute {
            current: 0,
            target: 2,
            backup: true,
            cut,
            corrupt_backup: cut != Cut::BeforeBackup,
            payload: 0,
            reopens: 1,
        });
    }
    cases
}
fn err(e: impl std::fmt::Debug) -> String {
    format!("{e:?}")
}
fn version(n: u64) -> Result<MigrationVersion, String> {
    MigrationVersion::new(n).map_err(err)
}
const SQL1: &str = "CREATE TABLE probe(step INTEGER PRIMARY KEY,value TEXT NOT NULL) STRICT; INSERT INTO probe VALUES(1,'one'); PRAGMA user_version=1;";
const SQL2: &str = "INSERT INTO probe VALUES(2,'two'); PRAGMA user_version=2;";
const SQL3: &str = "INSERT INTO probe VALUES(3,'three'); PRAGMA user_version=3;";
const BAD3: &str = "INSERT INTO probe VALUES(1,'duplicate'); PRAGMA user_version=3;";
static REGISTRIES: OnceLock<Result<[Vec<MigrationDescriptor>; 4], String>> = OnceLock::new();
fn registry(backup: bool, bad: bool) -> Result<MigrationRegistry, String> {
    let all = REGISTRIES
        .get_or_init(|| {
            let mut result: [Vec<MigrationDescriptor>; 4] = std::array::from_fn(|_| Vec::new());
            for (index, descriptors) in result.iter_mut().enumerate() {
                for (n, sql) in
                    [SQL1, SQL2, if index >= 2 { BAD3 } else { SQL3 }].into_iter().enumerate()
                {
                    descriptors.push(MigrationDescriptor::new(
                        version(n as u64 + 1)?,
                        "praxis-stage4",
                        sql,
                        peritus_codec::sha256(sql.as_bytes()),
                        if index % 2 == 1 {
                            BackupPolicy::Required
                        } else {
                            BackupPolicy::NotRequired
                        },
                        128,
                    ));
                }
            }
            Ok(result)
        })
        .as_ref()
        .map_err(Clone::clone)?;
    Ok(MigrationRegistry::from_static(&all[usize::from(backup) + 2 * usize::from(bad)]))
}

fn planner(
    current: u8,
    target: u8,
    minimum: u8,
    maximum: u8,
    database: u64,
    available: u64,
    reserve: u64,
) -> Result<crate::Metrics, String> {
    let compatibility =
        ApplicationCompatibility::new(u64::from(minimum), version(u64::from(maximum))?);
    if minimum > maximum {
        if compatibility.is_ok() {
            return Err("migration compatibility accepted reversed interval".into());
        }
        return Ok(crate::Metrics::from([
            ("rejected_plans".into(), 1),
            ("invalid_compatibility_ranges".into(), 1),
        ]));
    }
    let compatibility = compatibility.map_err(err)?;
    let registry = MigrationRegistry::current();
    // This is the supported compiled registry, not an archived-schema assumption.
    let sources = registry.descriptors();
    if sources.len() != 4
        || sources.iter().enumerate().any(|(i, d)| d.version().get() != i as u64 + 1)
    {
        return Err(
            "current migration registry changed: refresh explicit Stage4 version contract".into()
        );
    }
    let selected: Vec<_> =
        (1_u64..=4).filter(|v| *v > u64::from(current) && *v <= u64::from(target)).collect();
    let backup = !selected.is_empty();
    let scratch: u128 = selected.iter().map(|v| if *v == 1 { 65_536 } else { 1_048_576 }).sum();
    let required = u128::from(reserve) + scratch + if backup { u128::from(database) } else { 0 };
    let valid = target >= current
        && target <= 4
        && minimum <= current
        && target <= maximum
        && required <= u128::from(u64::MAX)
        && required <= u128::from(available);
    let result = MigrationPlan::from_observation(
        registry,
        u64::from(current),
        version(u64::from(target))?,
        compatibility,
        database,
        available,
        reserve,
    );
    if result.is_ok() != valid {
        return Err(format!(
            "migration interval/capacity oracle disagreement: current={current},target={target},required={required},available={available},result={result:?}"
        ));
    }
    if let Ok(plan) = result {
        let actual: Vec<_> = plan.steps().iter().map(|s| s.descriptor().version().get()).collect();
        if actual != selected
            || plan.backup_required() != backup
            || u128::from(plan.required_space_bytes()) != required
            || plan.current_version() != u64::from(current)
            || plan.target_version().get() != u64::from(target)
            || plan.observed_available_bytes() != available
        {
            return Err("migration selected interval or checked resource projection drift".into());
        }
    }
    let mut metrics = crate::Metrics::from([
        ("accepted_plans".into(), u64::from(valid)),
        ("rejected_plans".into(), u64::from(!valid)),
    ]);
    let interval = target >= current && target <= 4 && minimum <= current && target <= maximum;
    if interval {
        metrics.insert(format!("interval_{current}_to_{target}"), 1);
        metrics.insert(if backup { "nonempty_intervals" } else { "no_op_intervals" }.into(), 1);
        metrics.insert("tight_compatibility_lower".into(), u64::from(minimum == current));
        metrics.insert("tight_compatibility_upper".into(), u64::from(maximum == target));
        let class = if required > u128::from(u64::MAX) {
            "capacity_overflow"
        } else if required == u128::from(available) {
            "capacity_exact"
        } else if required == u128::from(available) + 1 {
            "capacity_one_byte_short"
        } else if required > u128::from(available) {
            "capacity_insufficient"
        } else {
            "capacity_spare"
        };
        metrics.insert(class.into(), 1);
        metrics.insert(
            "capacity_exact_u64_max".into(),
            u64::from(required == u128::from(u64::MAX) && available == u64::MAX),
        );
        metrics.insert("no_op_ignores_database_bytes".into(), u64::from(!backup && database > 0));
    } else {
        metrics.insert("invalid_version_intervals".into(), 1);
    }
    Ok(metrics)
}
struct Hooks {
    cut: Cut,
    step: u64,
    hits: u64,
}
impl MigrationHooks for Hooks {
    fn should_fail(&mut self, point: MigrationFailpoint) -> bool {
        let hit = match (self.cut, point) {
            (Cut::AfterRecord, MigrationFailpoint::AfterRecoveryRecord)
            | (Cut::BeforeBackup, MigrationFailpoint::BeforeBackup)
            | (Cut::AfterBackup, MigrationFailpoint::AfterBackup)
            | (Cut::BeforeCommit, MigrationFailpoint::BeforeCommit)
            | (Cut::AfterCommit, MigrationFailpoint::AfterCommit) => true,
            (Cut::BeforeStep(_), MigrationFailpoint::BeforeStep(v)) => v.get() == self.step,
            _ => false,
        };
        self.hits += u64::from(hit);
        hit
    }
}
fn config(database: &Path, backups: &Path, latest: u64) -> Result<MigrationConfig, String> {
    MigrationConfig::new(
        database,
        backups,
        "praxis-stage4",
        ApplicationCompatibility::new(0, version(latest)?).map_err(err)?,
        0,
    )
    .map(|c| c.with_busy_timeout(Duration::from_millis(200)))
    .map_err(err)
}
fn operation(n: u8) -> Result<MigrationOperationId, String> {
    MigrationOperationId::new([n; 16]).map_err(err)
}
fn inspect(database: &Path, wanted: u8, payload: u8) -> Result<(), String> {
    let connection =
        Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(err)?;
    let actual: i64 =
        connection.pragma_query_value(None, "user_version", |r| r.get(0)).map_err(err)?;
    if actual != i64::from(wanted) {
        return Err(format!("migration atomic version mismatch: wanted={wanted},actual={actual}"));
    }
    let sentinel: String = connection
        .query_row("SELECT value FROM sentinel WHERE id=1", [], |r| r.get(0))
        .map_err(err)?;
    if sentinel != format!("sentinel-{payload}-λ") {
        return Err("migration changed unrelated sentinel bytes".into());
    }
    let mut statement = connection
        .prepare("SELECT version FROM schema_migrations ORDER BY version")
        .map_err(err)?;
    let applied: Vec<i64> = statement
        .query_map([], |row| row.get(0))
        .map_err(err)?
        .collect::<Result<_, _>>()
        .map_err(err)?;
    if applied != (1..=i64::from(wanted)).collect::<Vec<_>>() {
        return Err("migration committed partial or repeated history".into());
    }
    if wanted > 0 {
        let mut statement =
            connection.prepare("SELECT step FROM probe ORDER BY step").map_err(err)?;
        let steps: Vec<i64> = statement
            .query_map([], |row| row.get(0))
            .map_err(err)?
            .collect::<Result<_, _>>()
            .map_err(err)?;
        if steps != (1..=i64::from(wanted)).collect::<Vec<_>>() {
            return Err("migration effects missing, duplicated, or partially committed".into());
        }
    }
    Ok(())
}
fn backup_path(directory: &Path, current: u8) -> std::path::PathBuf {
    directory.join(format!("migration-{}-from-{current}.sqlite3", "01".repeat(16)))
}

fn execute(
    current: u8,
    target: u8,
    backup: bool,
    cut: Cut,
    corrupt_backup: bool,
    payload: u8,
    reopens: u8,
) -> Result<crate::Metrics, String> {
    if target == 0
        || target > 3
        || current >= target
        || !(1..=3).contains(&reopens)
        || (!backup && matches!(cut, Cut::BeforeBackup | Cut::AfterBackup))
        || (corrupt_backup && (!backup || !matches!(cut, Cut::AfterBackup | Cut::BeforeCommit)))
        || (cut == Cut::SqlError && (target != 3 || !backup))
    {
        return Err("invalid migration execution fixture".into());
    }
    let directory =
        tempfile::Builder::new().prefix("peritus-stage4-migration-").tempdir().map_err(err)?;
    let database = directory.path().join("state.sqlite");
    let backups = directory.path().join("backups");
    {
        let connection = Connection::open(&database).map_err(err)?;
        connection
            .execute_batch(
                "CREATE TABLE sentinel(id INTEGER PRIMARY KEY,value TEXT NOT NULL) STRICT;",
            )
            .map_err(err)?;
        connection
            .execute("INSERT INTO sentinel VALUES(1,?1)", [format!("sentinel-{payload}-λ")])
            .map_err(err)?;
    }
    let selected_registry = registry(backup, cut == Cut::SqlError)?;
    let configuration = config(&database, &backups, 3)?;
    let mut engine =
        MigrationEngine::open(configuration.clone(), selected_registry).map_err(err)?;
    if current > 0 {
        let plan = engine.preflight(version(u64::from(current))?).map_err(err)?.into_plan();
        engine.apply(&plan, operation(10)?).map_err(err)?;
    }
    inspect(&database, current, payload)?;
    let plan = engine.preflight(version(u64::from(target))?).map_err(err)?.into_plan();
    let expected_steps: Vec<_> = (u64::from(current) + 1..=u64::from(target)).collect();
    if plan.steps().iter().map(|s| s.descriptor().version().get()).collect::<Vec<_>>()
        != expected_steps
        || plan.backup_required() != backup
    {
        return Err("real preflight interval differs from independent interval".into());
    }
    let step = if let Cut::BeforeStep(n) = cut {
        u64::from(current) + 1 + u64::from(n) % (u64::from(target - current))
    } else {
        0
    };
    let mut hooks = Hooks { cut, step, hits: 0 };
    let result = engine.apply_with_hooks(&plan, operation(1)?, &mut hooks);
    if cut == Cut::None {
        result.map_err(err)?;
    } else {
        let expected = if cut == Cut::AfterCommit {
            MigrationErrorCode::Indeterminate
        } else if cut == Cut::SqlError {
            MigrationErrorCode::ApplyFailed
        } else {
            MigrationErrorCode::InjectedFailure
        };
        let failure = result.err().ok_or("migration fault was not reached")?;
        if failure.code() != expected {
            return Err(format!(
                "wrong migration fault classification: cut={cut:?},actual={failure:?}"
            ));
        }
    }
    if !matches!(cut, Cut::None | Cut::SqlError) && hooks.hits != 1 {
        return Err("migration failpoint unreachable or repeated".into());
    }
    drop(engine);
    let committed = matches!(cut, Cut::None | Cut::AfterCommit);
    inspect(&database, if committed { target } else { current }, payload)?;
    let mut engine =
        MigrationEngine::open(configuration.clone(), selected_registry).map_err(err)?;
    let report = engine.reconcile().map_err(err)?;
    let id = operation(1)?;
    let expected_action = match cut {
        Cut::None => None,
        Cut::AfterRecord => Some(if backup {
            RecoveryAction::ResumeBackup(id)
        } else {
            RecoveryAction::ResumeApply(id)
        }),
        Cut::BeforeBackup => Some(RecoveryAction::ResumeBackup(id)),
        Cut::AfterBackup => Some(RecoveryAction::ResumeApply(id)),
        Cut::AfterCommit => Some(RecoveryAction::ReconciledApplied(id)),
        Cut::BeforeStep(_) | Cut::BeforeCommit | Cut::SqlError => Some(if backup {
            RecoveryAction::RestoreBackup(id)
        } else {
            RecoveryAction::RetryApply(id)
        }),
    };
    if report.actions() != expected_action.as_slice() {
        return Err(format!(
            "migration restart action mismatch: cut={cut:?},actual={:?}",
            report.actions()
        ));
    }
    inspect(&database, if committed { target } else { current }, payload)?;
    let mut backup_rejections = 0;
    let mut restores = 0;
    if corrupt_backup {
        let path = backup_path(&backups, current);
        let original = fs::read(&path).map_err(err)?;
        let mut corrupt = original.clone();
        let last = corrupt.last_mut().ok_or("empty migration backup")?;
        *last ^= 1;
        fs::write(&path, &corrupt).map_err(err)?;
        let rejection =
            engine.restore_backup(id).err().ok_or("corrupt migration backup was restored")?;
        if rejection.code() != MigrationErrorCode::BackupFailed {
            return Err(format!("corrupt backup gave unexpected error: {rejection:?}"));
        }
        inspect(&database, current, payload)?;
        let mut file = fs::OpenOptions::new().write(true).truncate(true).open(path).map_err(err)?;
        file.write_all(&original).map_err(err)?;
        file.sync_all().map_err(err)?;
        backup_rejections += 1;
    }
    match expected_action {
        Some(RecoveryAction::RestoreBackup(_)) => {
            let restored = engine.restore_backup(id).map_err(err)?;
            if restored.state() != RecoveryState::Restored {
                return Err("migration restore did not retain restored recovery state".into());
            }
            inspect(&database, current, payload)?;
            restores += 1;
            if engine.apply(&plan, id).is_ok() {
                return Err("restored migration reused the retired operation identity".into());
            }
            drop(engine);
            engine = MigrationEngine::open(configuration.clone(), registry(backup, false)?)
                .map_err(err)?;
            let retry = engine.preflight(version(u64::from(target))?).map_err(err)?.into_plan();
            engine.apply(&retry, operation(2)?).map_err(err)?;
        }
        Some(RecoveryAction::RetryApply(_)) => {
            if cut == Cut::SqlError {
                drop(engine);
                engine = MigrationEngine::open(configuration.clone(), registry(backup, false)?)
                    .map_err(err)?;
                if engine.apply(&plan, id).is_ok() {
                    return Err("changed SQL registry accepted a stale migration plan".into());
                }
                let retry = engine.preflight(version(u64::from(target))?).map_err(err)?.into_plan();
                engine.apply(&retry, operation(2)?).map_err(err)?;
            } else {
                engine.apply(&plan, id).map_err(err)?;
            }
        }
        Some(RecoveryAction::ResumeBackup(_) | RecoveryAction::ResumeApply(_)) => {
            engine.apply(&plan, id).map_err(err)?;
        }
        Some(RecoveryAction::ReconciledApplied(_)) | None => {
            if engine.apply(&plan, id).is_ok() {
                return Err("postcommit stale plan repeated migration effects".into());
            }
        }
    }
    inspect(&database, target, payload)?;
    for _ in 0..reopens {
        drop(engine);
        engine =
            MigrationEngine::open(configuration.clone(), registry(backup, false)?).map_err(err)?;
        if !engine.reconcile().map_err(err)?.actions().is_empty() {
            return Err("settled migration retained pending recovery work".into());
        }
        inspect(&database, target, payload)?;
    }
    let no_op = engine.preflight(version(u64::from(target))?).map_err(err)?.into_plan();
    if !no_op.steps().is_empty() || no_op.backup_required() {
        return Err("idempotent current-version plan acquired migration work".into());
    }
    engine.apply(&no_op, operation(3)?).map_err(err)?;
    inspect(&database, target, payload)?;
    Ok(crate::Metrics::from([
        ("executions".into(), 1),
        ("fault_cuts".into(), u64::from(cut != Cut::None)),
        ("committed_after_interruption".into(), u64::from(cut == Cut::AfterCommit)),
        ("restores".into(), restores),
        ("corrupt_backup_rejections".into(), backup_rejections),
        ("reopens".into(), u64::from(reopens) + 1),
        ("backup_required_cases".into(), u64::from(backup)),
        ("explicit_retries".into(), u64::from(!committed)),
    ]))
}
fn current_install(reopens: u8) -> Result<crate::Metrics, String> {
    if !(1..=3).contains(&reopens) {
        return Err("invalid migration adoption fixture".into());
    }
    let directory = tempfile::tempdir().map_err(err)?;
    let database = directory.path().join("journal.sqlite");
    let store = peritus_journal::SqliteJournal::open(
        &database,
        peritus_journal::StoreId::new([89; 16]).map_err(err)?,
        peritus_journal::SqliteJournalOptions::default(),
    )
    .map_err(err)?;
    drop(store);
    let registry = MigrationRegistry::current();
    let latest = registry.latest().map_err(err)?;
    if latest.get() != 4 {
        return Err("compiled journal/migration current-install contract changed".into());
    }
    let configuration = config(&database, &directory.path().join("backups"), latest.get())?;
    let mut engine = MigrationEngine::open(configuration.clone(), registry).map_err(err)?;
    if !engine.adopt_current_install(operation(11)?).map_err(err)? {
        return Err("direct current journal installation was not adopted".into());
    }
    for _ in 0..reopens {
        drop(engine);
        engine = MigrationEngine::open(configuration.clone(), registry).map_err(err)?;
        if engine.adopt_current_install(operation(12)?).map_err(err)?
            || !engine.reconcile().map_err(err)?.actions().is_empty()
        {
            return Err(
                "current journal schema adoption repeated or invented migration recovery".into()
            );
        }
        let plan = engine.preflight(latest).map_err(err)?.into_plan();
        if plan.current_version() != 4 || !plan.steps().is_empty() {
            return Err("adopted current journal version drift".into());
        }
    }
    Ok(crate::Metrics::from([
        ("current_install_adoptions".into(), 1),
        ("reopens".into(), u64::from(reopens)),
    ]))
}
pub fn check(case: &Case) -> Result<crate::Metrics, String> {
    match *case {
        Case::Plan { current, target, minimum, maximum, database, available, reserve } => {
            planner(current, target, minimum, maximum, database, available, reserve)
        }
        Case::Execute { current, target, backup, cut, corrupt_backup, payload, reopens } => {
            execute(current, target, backup, cut, corrupt_backup, payload, reopens)
        }
        Case::CurrentInstall { reopens } => current_install(reopens),
    }
}
