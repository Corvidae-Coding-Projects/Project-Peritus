//! Read-only reconciliation and explicit human acknowledgement of unprovable outcomes.
use crate::{
    daemon,
    error::{Result, problem},
    files,
    state::App,
};
use serde_json::{Value, json};

pub fn pending(app: &App) -> Result<Value> {
    Ok(json!(app.snapshot()?.operations.iter().filter(|(_,r)|r.result.is_none()&&r.input["command"]!="daemon-request").map(|(id,r)|json!({"operation":id,"command":r.input["command"],"session":r.input["session"],"project":r.input["project"],"path":r.input["path"]})).collect::<Vec<_>>()))
}
pub fn observe(app: &App, id: &str) -> Result<Value> {
    let Some(mut record) = app.snapshot()?.operations.get(id).cloned() else {
        return Ok(Value::Null);
    };
    if record.result.is_none() {
        let result = if record.input["command"] == "file-save" {
            let check = || -> Result<Option<Value>> {
                let root = app.project(record.input["project"].as_str().unwrap_or(""))?.root;
                let path = files::resolve(&root, record.input["path"].as_str().unwrap_or(""))?;
                let bytes = files::read_text(&path)?;
                let hash = files::revision(&bytes);
                Ok((record.input["hash"] == hash)
                    .then(|| json!({"revision":hash,"bytes":bytes.len(),"recovered":true})))
            };
            check().unwrap_or(None)
        } else {
            daemon::receipts::observed(app, id)?
        };
        if let Some(result) = result {
            app.update(|state| {
                state.operations.get_mut(id).ok_or_else(|| problem("Operation missing"))?.result =
                    Some(result.clone());
                Ok(())
            })?;
            record.result = Some(result);
        }
    }
    Ok(json!(record))
}
pub fn acknowledge(app: &App, id: &str) -> Result<Value> {
    let observed = observe(app, id)?;
    if !observed["result"].is_null() {
        return Ok(observed);
    }
    app.update(|state|{
        let record=state.operations.get_mut(id).ok_or_else(||problem("Operation not found"))?;
        if record.input["command"]=="daemon-request"{return Err(problem("Review the parent operation, not its transport record"));}
        record.result=Some(json!({"reviewed":true,"error":"Outcome manually reviewed by the user. No acceptance was inferred and the operation was not repeated."}));
        Ok(())
    })?;
    observe(app, id)
}
