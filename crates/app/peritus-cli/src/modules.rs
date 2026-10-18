//! Explicit local module management; package installation never executes code.
use crate::error::CliError;
use peritus_modules::Registry;
use serde_json::{Value, json};
use std::path::Path;

pub(crate) fn execute(arguments: &[String]) -> Result<Value, CliError> {
    let usage = || {
        CliError::usage(
            "usage: peritus modules <list|market|refresh|show ID|install DIRECTORY|install-market ID DIGEST|enable ID|disable ID|remove ID>",
        )
    };
    let registry = Registry::for_user()
        .map_err(|e| CliError::runtime("open module registry", e.to_string()))?;
    let result=match arguments.iter().map(String::as_str).collect::<Vec<_>>().as_slice(){
        []|["list"]=>registry.list(),
        ["market"]=>registry.market(),
        ["refresh"]=>registry.refresh_market(),
        ["install-market",id,digest]=>registry.install_market(id,digest),
        ["show",id]=>registry.load(id,false).and_then(|d|Ok(json!({"manifest":d.manifest,"digest":d.digest,"bundled":d.bundled,"directory":d.root}))),
        ["install",directory]=>registry.install(Path::new(directory)),
        ["enable",id]=>registry.enable(id,true),
        ["disable",id]=>registry.enable(id,false),
        ["remove",id]=>registry.remove(id),
        _=>return Err(usage()),
    };
    result.map_err(|e| CliError::runtime("module operation", e.to_string()))
}
