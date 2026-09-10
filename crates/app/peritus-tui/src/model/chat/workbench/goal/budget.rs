//! Local parsing for explicit persistent-goal budget edits.

use peritus_app_protocol::WorkbenchGoalBudget;

pub(super) fn parse_budget(
    arguments: &str,
    current: WorkbenchGoalBudget,
) -> Result<WorkbenchGoalBudget, &'static str> {
    if arguments == "none" {
        return Ok(WorkbenchGoalBudget::default());
    }
    let mut active = current.max_active_millis();
    let mut requests = current.max_requests();
    let mut tools = current.max_tool_calls();
    let mut tokens = current.max_total_tokens();
    let mut seen = [false; 4];
    for field in arguments.split_whitespace() {
        let Some((name, value)) = field.split_once('=') else { return Err(budget_help()) };
        let (index, slot) = match name {
            "time" | "active" => (0, 0),
            "requests" => (1, 1),
            "tools" => (2, 2),
            "tokens" => (3, 3),
            "cost" => {
                return Err(
                    "Provider cost limits are not available because cost provenance can be unknown; no budget changed.",
                );
            }
            _ => return Err(budget_help()),
        };
        if seen[index] {
            return Err("Each budget field may be specified once; no budget changed.");
        }
        seen[index] = true;
        match slot {
            0 => active = optional(value, parse_duration)?,
            1 => requests = optional(value, |value| value.parse::<u32>().ok())?,
            2 => tools = optional(value, |value| value.parse::<u32>().ok())?,
            3 => tokens = optional(value, |value| value.parse::<u64>().ok())?,
            _ => unreachable!(),
        }
    }
    WorkbenchGoalBudget::new(active, requests, tools, tokens).map_err(|_| {
        "Budget limits must be positive integers; use field=none to return a field to the host ceiling."
    })
}

fn optional<T>(
    value: &str,
    parse: impl FnOnce(&str) -> Option<T>,
) -> Result<Option<T>, &'static str> {
    if value == "none" { Ok(None) } else { parse(value).map(Some).ok_or_else(budget_help) }
}

fn parse_duration(value: &str) -> Option<u64> {
    let (number, multiplier) = [("ms", 1), ("s", 1_000), ("m", 60_000), ("h", 3_600_000)]
        .into_iter()
        .find_map(|(suffix, multiplier)| {
            value.strip_suffix(suffix).map(|number| (number, multiplier))
        })
        .unwrap_or((value, 1));
    number.parse::<u64>().ok()?.checked_mul(multiplier)
}

const fn budget_help() -> &'static str {
    "Use /budget [none | time=<N>[ms|s|m|h] requests=<N> tools=<N> tokens=<N>]; field=none restores the host ceiling."
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_edits_fields_without_resetting_omitted_limits() {
        let current = WorkbenchGoalBudget::new(Some(1_000), Some(2), Some(3), None).unwrap();
        let value = parse_budget("time=2m requests=5 tools=none tokens=900", current).unwrap();
        assert_eq!(value.max_active_millis(), Some(120_000));
        assert_eq!(value.max_requests(), Some(5));
        assert_eq!(value.max_tool_calls(), None);
        assert_eq!(value.max_total_tokens(), Some(900));
        assert!(parse_budget("cost=5", current).is_err());
        assert!(parse_budget("requests=0", current).is_err());
    }
}
