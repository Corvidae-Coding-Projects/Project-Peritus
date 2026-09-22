//! Explicit suggestion inbox commands; no implicit evaluation.

use super::{Command, Parser, required};
use crate::{
    error::CliError,
    id::{parse_hex_digest, parse_hex_id},
};
use peritus_app_protocol::{
    ImprovementRequest, ImprovementText, ProductProviderSelection, ProductRunRequest,
};
use peritus_types::{ProviderProfileId, RunId, Sha256Digest, WorkspaceId};
use std::collections::BTreeMap;

pub(super) fn parse(parser: &mut Parser) -> Result<Command, CliError> {
    let action = parser.command("improvements subcommand")?;
    let allowed: &[&str] = match action.as_str() {
        "list" => &["--workspace"],
        "suggest" => &["--workspace", "--run", "--proposal"],
        "dismiss" => &["--workspace", "--candidate"],
        "evaluate" => &["--workspace", "--candidate", "--target", "--provider", "--run"],
        _ => {
            return Err(CliError::usage(
                "Expected improvements list, suggest, dismiss, or evaluate",
            ));
        }
    };
    let mut options = BTreeMap::new();
    while let Some(option) = parser.peek_utf8()? {
        if !allowed.contains(&option) {
            return Err(CliError::usage(format!("Unknown improvements option: {option}")));
        }
        let option = parser.command("option")?;
        let value = parser.value_utf8(&option)?;
        if options.insert(option.clone(), value).is_some() {
            return Err(CliError::usage(format!("Duplicate option: {option}")));
        }
    }
    let value = |key: &str| required(options.get(key).cloned(), key);
    let id = |key: &str| parse_hex_id(&value(key)?, key);
    let workspace = WorkspaceId::new(id("--workspace")?)
        .map_err(|_| CliError::usage("Invalid workspace identity"))?;
    let candidate = || {
        value("--candidate")
            .and_then(|s| parse_hex_digest(&s, "--candidate"))
            .map(Sha256Digest::new)
    };
    let request = match action.as_str() {
        "list" => ImprovementRequest::List(workspace),
        "suggest" => ImprovementRequest::Suggest {
            workspace,
            run: RunId::new(id("--run")?).map_err(|_| CliError::usage("Invalid source run"))?,
            proposal: ImprovementText::new(value("--proposal")?)
                .map_err(|e| CliError::usage(e.to_string()))?,
        },
        "dismiss" => ImprovementRequest::Dismiss { workspace, candidate: candidate()? },
        "evaluate" => {
            let target = WorkspaceId::new(id("--target")?)
                .map_err(|_| CliError::usage("Invalid target workspace"))?;
            let provider = ProviderProfileId::new(id("--provider")?)
                .map_err(|_| CliError::usage("Invalid provider identity"))?;
            let run = RunId::new(id("--run")?)
                .map_err(|_| CliError::usage("Invalid evaluation run identity"))?;
            ImprovementRequest::Evaluate {
                workspace,
                candidate: candidate()?,
                run: ProductRunRequest::new(
                    run,
                    target,
                    ProductProviderSelection::new(provider, provider, provider),
                    "Evaluate selected harness suggestion".into(),
                )
                .map_err(|e| CliError::usage(e.to_string()))?,
            }
        }
        _ => unreachable!("action checked above"),
    };
    Ok(Command::Improvements(request))
}
