//! Header-only Google rejection metadata normalization.

use std::time::{SystemTime, UNIX_EPOCH};

use peritus_model_protocol::{
    OptionalObservation, OptionalObservationKind, OptionalObservationStatus, ProtocolLimits,
    ResponseId, RetryAfterObservation, RetryAfterParseStatus, RetryAfterUnit,
};
use peritus_provider_core::{HttpHeaders, ProviderCoreError};

const MAX_RESPONSE_ID_BYTES: usize = 512;

pub(super) struct GoogleRetryAfter {
    pub(super) delay_millis: Option<u64>,
    pub(super) observation: Option<RetryAfterObservation>,
}

pub(super) struct GoogleResponseIdentity {
    pub(super) response_id: Option<ResponseId>,
    pub(super) observation: Option<OptionalObservation>,
}

pub(super) fn retry_after(
    headers: &HttpHeaders,
    limits: ProtocolLimits,
) -> Result<GoogleRetryAfter, ProviderCoreError> {
    let Some(value) = headers.first("retry-after") else {
        return Ok(GoogleRetryAfter { delay_millis: None, observation: None });
    };
    let Some(raw_value) = value.nonsensitive_bytes() else {
        return Ok(GoogleRetryAfter { delay_millis: None, observation: None });
    };
    parse_retry_after(raw_value, limits)
}

pub(super) fn response_identity(headers: &HttpHeaders) -> GoogleResponseIdentity {
    for name in ["x-goog-request-id", "x-request-id"] {
        let Some(value) = headers.first(name) else { continue };
        let Some(bytes) = value.nonsensitive_bytes() else { continue };
        if bytes.len() > MAX_RESPONSE_ID_BYTES {
            return rejected_identity(OptionalObservationStatus::ExceededBound, bytes);
        }
        let Ok(text) = core::str::from_utf8(bytes) else {
            return rejected_identity(OptionalObservationStatus::InvalidEncoding, bytes);
        };
        return match ResponseId::new(text.to_owned()) {
            Ok(response_id) => GoogleResponseIdentity {
                response_id: Some(response_id),
                observation: None,
            },
            Err(_) => rejected_identity(OptionalObservationStatus::InvalidValue, bytes),
        };
    }
    GoogleResponseIdentity { response_id: None, observation: None }
}

fn rejected_identity(
    status: OptionalObservationStatus,
    bytes: &[u8],
) -> GoogleResponseIdentity {
    GoogleResponseIdentity {
        response_id: None,
        observation: Some(OptionalObservation::new(
            OptionalObservationKind::MappedRequestId,
            status,
            bytes,
        )),
    }
}

fn parse_retry_after(
    raw_value: &[u8],
    limits: ProtocolLimits,
) -> Result<GoogleRetryAfter, ProviderCoreError> {
    let Ok(text) = core::str::from_utf8(raw_value) else {
        return unschedulable(
            raw_value,
            RetryAfterUnit::Unsupported,
            RetryAfterParseStatus::Invalid,
            limits,
        );
    };
    let value = text.trim_matches(|character| matches!(character, ' ' | '\t'));
    match decimal_delay_millis(value) {
        DecimalDelay::Parsed { delay_millis, fractional } => {
            let legacy_delta = !fractional
                && raw_value.len() <= 64
                && value == text
                && delay_millis <= 86_400_000;
            let observation = (!legacy_delta)
                .then(|| {
                    observation(
                        raw_value,
                        RetryAfterUnit::DeltaSeconds,
                        RetryAfterParseStatus::Parsed,
                        None,
                        limits,
                    )
                })
                .transpose()?;
            return Ok(GoogleRetryAfter {
                delay_millis: Some(delay_millis),
                observation,
            });
        }
        DecimalDelay::Unrepresentable => {
            return unschedulable(
                raw_value,
                RetryAfterUnit::DeltaSeconds,
                RetryAfterParseStatus::Unrepresentable,
                limits,
            );
        }
        DecimalDelay::NotDecimal => {}
    }
    let Ok(eligible) = httpdate::parse_http_date(value) else {
        return unschedulable(
            raw_value,
            RetryAfterUnit::Unsupported,
            RetryAfterParseStatus::Invalid,
            limits,
        );
    };
    parsed_http_date(raw_value, eligible, limits)
}

enum DecimalDelay {
    NotDecimal,
    Parsed { delay_millis: u64, fractional: bool },
    Unrepresentable,
}

fn decimal_delay_millis(value: &str) -> DecimalDelay {
    let (whole, fraction) = value
        .split_once('.')
        .map_or((value, None), |(whole, fraction)| (whole, Some(fraction)));
    if whole.is_empty() || !whole.bytes().all(|byte| byte.is_ascii_digit()) {
        return DecimalDelay::NotDecimal;
    }
    let fractional = fraction.is_some();
    if fraction.is_some_and(|value| {
        value.is_empty()
            || value.contains('.')
            || !value.bytes().all(|byte| byte.is_ascii_digit())
    }) {
        return DecimalDelay::NotDecimal;
    }
    let Ok(seconds) = whole.parse::<u64>() else {
        return DecimalDelay::Unrepresentable;
    };
    let Some(mut delay_millis) = seconds.checked_mul(1_000) else {
        return DecimalDelay::Unrepresentable;
    };
    if let Some(fraction) = fraction {
        let bytes = fraction.as_bytes();
        let mut millis = 0_u64;
        for index in 0..3 {
            millis = millis.saturating_mul(10);
            if let Some(digit) = bytes.get(index) {
                millis = millis.saturating_add(u64::from(*digit - b'0'));
            }
        }
        if bytes.get(3..).is_some_and(|tail| tail.iter().any(|digit| *digit != b'0')) {
            millis = millis.saturating_add(1);
        }
        let Some(value) = delay_millis.checked_add(millis) else {
            return DecimalDelay::Unrepresentable;
        };
        delay_millis = value;
    }
    DecimalDelay::Parsed { delay_millis, fractional }
}

fn parsed_http_date(
    raw_value: &[u8],
    eligible: SystemTime,
    limits: ProtocolLimits,
) -> Result<GoogleRetryAfter, ProviderCoreError> {
    let eligible_unix_millis = match eligible.duration_since(UNIX_EPOCH) {
        Ok(duration) => match u64::try_from(duration.as_millis()) {
            Ok(value) => value,
            Err(_) => {
                return unschedulable(
                    raw_value,
                    RetryAfterUnit::HttpDate,
                    RetryAfterParseStatus::Unrepresentable,
                    limits,
                );
            }
        },
        Err(_) => 0,
    };
    let Some(observed_unix_millis) = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
    else {
        return unschedulable(
            raw_value,
            RetryAfterUnit::HttpDate,
            RetryAfterParseStatus::Unrepresentable,
            limits,
        );
    };
    let parse_status = if eligible_unix_millis <= observed_unix_millis {
        RetryAfterParseStatus::AlreadyEligible
    } else {
        RetryAfterParseStatus::Parsed
    };
    Ok(GoogleRetryAfter {
        delay_millis: Some(eligible_unix_millis.saturating_sub(observed_unix_millis)),
        observation: Some(observation(
            raw_value,
            RetryAfterUnit::HttpDate,
            parse_status,
            Some(eligible_unix_millis),
            limits,
        )?),
    })
}

fn unschedulable(
    raw_value: &[u8],
    unit: RetryAfterUnit,
    parse_status: RetryAfterParseStatus,
    limits: ProtocolLimits,
) -> Result<GoogleRetryAfter, ProviderCoreError> {
    Ok(GoogleRetryAfter {
        delay_millis: None,
        observation: Some(observation(raw_value, unit, parse_status, None, limits)?),
    })
}

fn observation(
    raw_value: &[u8],
    unit: RetryAfterUnit,
    parse_status: RetryAfterParseStatus,
    eligible_unix_millis: Option<u64>,
    limits: ProtocolLimits,
) -> Result<RetryAfterObservation, ProviderCoreError> {
    RetryAfterObservation::new(
        raw_value,
        unit,
        parse_status,
        eligible_unix_millis,
        limits,
    )
    .map_err(|_| {
        ProviderCoreError::malformed_stream(
            "google_retry_after",
            "Google retry-after observation was inconsistent",
        )
    })
}
