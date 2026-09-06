//! Closed argument parsing for CI and native-qualification shards.

use std::ffi::OsString;

use super::Command;
use crate::error::XtaskError;
use crate::product_package::qualification::QualificationInput;

pub(super) fn parse(
    first: Option<&OsString>,
    arguments: &mut impl Iterator<Item = OsString>,
) -> Result<Option<Command>, XtaskError> {
    match first.and_then(|value| value.to_str()) {
        Some("ci-shard") => parse_ci(arguments).map(Some),
        Some("product-native-qualification-shard") => {
            parse_h2(arguments, QualificationInput::Build).map(Some)
        }
        Some("product-native-qualification-prepared-shard") => {
            parse_h2(arguments, QualificationInput::Prepared).map(Some)
        }
        _ => Ok(None),
    }
}

fn parse_ci(arguments: &mut impl Iterator<Item = OsString>) -> Result<Command, XtaskError> {
    let operation = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .and_then(|value| crate::ci_shard::Operation::parse(&value))
        .ok_or_else(|| {
            XtaskError::invocation("ci-shard requires one supported operation and shard")
        })?;
    let shard = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .and_then(|value| crate::ci_shard::SHARD_NAMES.iter().copied().find(|name| *name == value))
        .ok_or_else(|| XtaskError::invocation("ci-shard requires one supported shard"))?;
    if arguments.next().is_some() {
        return Err(XtaskError::invocation("ci-shard accepts exactly two arguments"));
    }
    Ok(Command::CiShard { operation, shard })
}

fn parse_h2(
    arguments: &mut impl Iterator<Item = OsString>,
    input: QualificationInput,
) -> Result<Command, XtaskError> {
    let index = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|index| *index < crate::product_package::H2_SHARD_COUNT)
        .ok_or_else(|| {
            XtaskError::invocation(format!(
                "product-native-qualification-shard requires an index from 0 through {}",
                crate::product_package::H2_SHARD_COUNT - 1
            ))
        })?;
    if arguments.next().is_some() {
        return Err(XtaskError::invocation(
            "product-native-qualification-shard accepts exactly one index",
        ));
    }
    Ok(Command::ProductNativeQualificationShard { index, input })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shard_arguments_are_closed_and_bounded() {
        let mut ci = [OsString::from("test"), OsString::from("app-runner")].into_iter();
        assert_eq!(
            parse(Some(&OsString::from("ci-shard")), &mut ci).expect("CI shard").unwrap(),
            Command::CiShard { operation: crate::ci_shard::Operation::Test, shard: "app-runner" }
        );
        let mut h2 = std::iter::once(OsString::from("17"));
        assert_eq!(
            parse(Some(&OsString::from("product-native-qualification-shard")), &mut h2)
                .expect("H2 shard")
                .unwrap(),
            Command::ProductNativeQualificationShard {
                index: 17,
                input: QualificationInput::Build
            }
        );
        let mut invalid = std::iter::once(OsString::from("18"));
        assert!(
            parse(Some(&OsString::from("product-native-qualification-shard")), &mut invalid,)
                .is_err()
        );
    }

    #[test]
    fn prepared_shards_are_explicit_and_keep_the_same_argument_bounds() {
        for index in 0..18 {
            let mut arguments = std::iter::once(OsString::from(index.to_string()));
            assert_eq!(
                parse(
                    Some(&OsString::from("product-native-qualification-prepared-shard")),
                    &mut arguments
                )
                .expect("prepared shard"),
                Some(Command::ProductNativeQualificationShard {
                    index,
                    input: QualificationInput::Prepared
                })
            );
        }
        for values in [vec![], vec!["18"], vec!["-1"], vec!["0", "extra"]] {
            let mut arguments = values.into_iter().map(OsString::from);
            assert!(
                parse(
                    Some(&OsString::from("product-native-qualification-prepared-shard")),
                    &mut arguments
                )
                .is_err()
            );
        }
    }
}
