//! Real native consumer of the compiled skill catalog and production paginated readers.

use std::{error::Error, io::{self, BufRead, Write as _}};
use peritus_product_runner::bundled_skills;
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn Error>> {
    let mut output = io::BufWriter::new(io::stdout().lock());
    for line in io::stdin().lock().lines() {
        let request: Value = serde_json::from_str(&line?)?;
        let skill = request["skill"].as_str();
        let offset = request["offset"].as_u64().unwrap_or(0);
        let maximum = request["max_bytes"].as_u64().unwrap_or(1024) as usize;
        let result = match request["op"].as_str() {
            Some("inventory") => Ok(json!(bundled_skills::catalog().iter().map(|skill| json!({
                "skill": skill.name(), "description": skill.description(),
                "resources": skill.resources().iter().map(|resource| json!({
                    "path": resource.path(), "total_bytes": resource.bytes().len()
                })).collect::<Vec<_>>()
            })).collect::<Vec<_>>())),
            Some("list") => bundled_skills::catalog_page(skill, offset, maximum),
            Some("read") => bundled_skills::read_page(
                skill.unwrap_or(""), request["path"].as_str().unwrap_or(""), offset, maximum
            ),
            _ => return Err("unknown consumer operation".into()),
        };
        let response = result.unwrap_or_else(|error| json!({"error": error.to_string()}));
        serde_json::to_writer(&mut output, &response)?;
        output.write_all(b"\n")?;
        output.flush()?;
    }
    Ok(())
}
