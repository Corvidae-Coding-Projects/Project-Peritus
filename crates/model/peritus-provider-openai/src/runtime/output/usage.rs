//! Native thread totals become request-local usage before host accounting.

use super::DecodeFailure;
use peritus_model_protocol::UsageCounters;
use serde_json::Value;

pub(in crate::runtime) fn decode(value: Option<&Value>) -> Result<UsageCounters, DecodeFailure> {
    let Some(value) = value else { return Ok(UsageCounters::default()) };
    let object = value.as_object().ok_or(DecodeFailure::InvalidUsage)?;
    let input = optional_u64(object.get("input_tokens"))?;
    let cached = optional_u64(object.get("cached_input_tokens"))?;
    let output = optional_u64(object.get("output_tokens"))?;
    let total = optional_u64(object.get("total_tokens"))?;
    if matches!((input, output, total), (Some(left), Some(right), Some(sum)) if left.checked_add(right) != Some(sum))
    {
        return Err(DecodeFailure::InvalidUsage);
    }
    Ok(UsageCounters::new(input, cached, None, output, None, None, total, None))
}

pub(in crate::runtime) fn high_water(left: UsageCounters, right: UsageCounters) -> UsageCounters {
    UsageCounters::new(
        left.input_tokens().max(right.input_tokens()),
        left.cached_input_tokens().max(right.cached_input_tokens()),
        None,
        left.output_tokens().max(right.output_tokens()),
        None,
        None,
        left.total_tokens().max(right.total_tokens()),
        None,
    )
}

pub(super) fn relative(
    current: UsageCounters,
    previous: UsageCounters,
) -> Result<UsageCounters, DecodeFailure> {
    let input = difference(current.input_tokens(), previous.input_tokens())?;
    let cached = difference(current.cached_input_tokens(), previous.cached_input_tokens())?;
    let output = difference(current.output_tokens(), previous.output_tokens())?;
    let mut total = difference(current.total_tokens(), previous.total_tokens())?;
    // Optional counters can be reported on different turns. Do not claim a total for
    // a different interval from the independently retained input/output high waters.
    if matches!((input, output, total), (Some(left), Some(right), Some(sum)) if left.checked_add(right) != Some(sum))
    {
        total = None;
    }
    Ok(UsageCounters::new(input, cached, None, output, None, None, total, None))
}

fn difference(current: Option<u64>, previous: Option<u64>) -> Result<Option<u64>, DecodeFailure> {
    current
        .map(|value| value.checked_sub(previous.unwrap_or(0)).ok_or(DecodeFailure::InvalidUsage))
        .transpose()
}

fn optional_u64(value: Option<&Value>) -> Result<Option<u64>, DecodeFailure> {
    value.map_or(Ok(None), |value| value.as_u64().map(Some).ok_or(DecodeFailure::InvalidUsage))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn resumed_usage_charges_only_the_new_native_turn() {
        let first = decode(Some(
            &json!({"input_tokens":18744,"output_tokens":246,"cached_input_tokens":0}),
        ))
        .unwrap();
        let cumulative = decode(Some(
            &json!({"input_tokens":52438,"output_tokens":477,"cached_input_tokens":100}),
        ))
        .unwrap();
        let next = relative(cumulative, first).unwrap();
        assert_eq!(next.input_tokens(), Some(33694));
        assert_eq!(next.output_tokens(), Some(231));
        assert_eq!(next.cached_input_tokens(), Some(100));
        assert_eq!(next.total_tokens(), None);
        assert_eq!(first.input_tokens().unwrap() + next.input_tokens().unwrap(), 52438);
    }

    #[test]
    fn missing_usage_stays_unknown_and_counter_regression_is_rejected() {
        let previous = decode(Some(&json!({"input_tokens":12,"output_tokens":3}))).unwrap();
        assert_eq!(relative(UsageCounters::default(), previous).unwrap(), UsageCounters::default());
        let regressed = decode(Some(&json!({"input_tokens":11,"output_tokens":4}))).unwrap();
        assert_eq!(relative(regressed, previous), Err(DecodeFailure::InvalidUsage));
    }

    #[test]
    fn asynchronously_reported_totals_do_not_claim_a_different_interval() {
        let earlier =
            decode(Some(&json!({"input_tokens":10,"output_tokens":2,"total_tokens":12}))).unwrap();
        let later = decode(Some(&json!({"input_tokens":20,"output_tokens":4}))).unwrap();
        let current =
            decode(Some(&json!({"input_tokens":30,"output_tokens":6,"total_tokens":36}))).unwrap();
        let next = relative(current, high_water(earlier, later)).unwrap();
        assert_eq!(next.input_tokens(), Some(10));
        assert_eq!(next.output_tokens(), Some(2));
        assert_eq!(next.total_tokens(), None);
    }

    #[test]
    fn success_and_malformed_results_both_expose_only_new_usage() {
        let bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/fixtures/v1/runtime-success.jsonl"
        ));
        let previous = decode(Some(
            &json!({"input_tokens":10,"cached_input_tokens":1,"output_tokens":3,"total_tokens":13}),
        ))
        .unwrap();
        for final_message in [None, Some("invalid structured result")] {
            let decoded = crate::runtime::output::decode_with_usage(
                bytes,
                &std::collections::BTreeSet::new(),
                0..=0,
                final_message,
            )
            .relative_to(previous);
            assert_eq!(decoded.usage.input_tokens(), Some(2));
            assert_eq!(decoded.usage.output_tokens(), Some(2));
            assert_eq!(decoded.usage.cached_input_tokens(), Some(1));
            assert_eq!(decoded.usage.total_tokens(), Some(4));
            match final_message {
                None => assert_eq!(decoded.turn.unwrap().usage, decoded.usage),
                Some(_) => assert!(decoded.turn.is_err()),
            }
        }
    }
}
