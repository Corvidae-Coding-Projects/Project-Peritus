//! One-command native H1 release qualification.

use std::env;

use peritus_resilience::{
    NativeControllerLimits, NativeResilienceFactory, QualificationConfig, QualificationRunner,
    QualificationText, ScenarioCatalog, SubjectDescriptor, SubjectId, render_report_json,
};

use crate::args::OperatorOptions;
use crate::{digest, publication};

/// Terminal status of a completed and retained H1 campaign.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum H1OperatorStatus {
    /// Every production scenario passed against the exact candidate with complete cleanup.
    Ready,
    /// The explicitly selected diagnostic case passed without claiming production readiness.
    DiagnosticPassed,
    /// The report was retained honestly, but one or more H1 obligations failed.
    NotReady,
}

/// Runs the complete production H1 catalog and atomically retains its full report.
///
/// # Errors
///
/// Returns syntax, identity, filesystem, controller, catalog, or report-publication failures.
pub fn run_h1_operator() -> Result<H1OperatorStatus, Box<dyn std::error::Error>> {
    let arguments = env::args_os().skip(1).collect::<Vec<_>>();
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    runtime.block_on(run(&arguments))
}

async fn run(
    arguments: &[std::ffi::OsString],
) -> Result<H1OperatorStatus, Box<dyn std::error::Error>> {
    let options = OperatorOptions::parse(arguments)?;
    let descriptor = SubjectDescriptor::new(
        SubjectId::new(options.subject_id)?,
        QualificationText::new(options.implementation)?,
        digest::file(&options.candidate)?,
    );
    let config = QualificationConfig::default();
    let catalog = selected_catalog(options.scenario_id.as_deref())?;
    let needs_reboot_image =
        options.scenario_id.as_deref().is_none_or(|scenario| scenario.starts_with("h1.reboot."));
    if needs_reboot_image && options.reboot_image.is_none() {
        return Err("production or reboot-focused H1 qualification requires --reboot-image".into());
    }
    let mut factory = NativeResilienceFactory::new(
        &options.controller,
        &options.candidate,
        &options.scratch,
        &options.artifacts,
        descriptor,
        config,
        NativeControllerLimits::default(),
    )?;
    if let Some(image) = &options.reboot_image {
        factory = factory.with_controller_resource(image)?;
    }
    let report = QualificationRunner::run(config, &catalog, &factory).await;
    publication::publish(&options.report, &render_report_json(&report)?)?;
    Ok(if report.is_ready() {
        H1OperatorStatus::Ready
    } else if options.scenario_id.is_some() && report.summary().passed() == 1 {
        H1OperatorStatus::DiagnosticPassed
    } else {
        H1OperatorStatus::NotReady
    })
}

fn selected_catalog(
    scenario_id: Option<&str>,
) -> Result<ScenarioCatalog, Box<dyn std::error::Error>> {
    let production = ScenarioCatalog::h1_production()?;
    let Some(scenario_id) = scenario_id else {
        return Ok(production);
    };
    let scenario = production
        .scenarios()
        .iter()
        .find(|scenario| scenario.id().as_str() == scenario_id)
        .cloned()
        .ok_or_else(|| format!("unknown production H1 diagnostic scenario: {scenario_id}"))?;
    Ok(ScenarioCatalog::custom(vec![scenario])?)
}
