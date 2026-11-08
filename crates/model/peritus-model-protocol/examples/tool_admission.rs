//! Public-consumer replay of host admission and provider projection boundaries.
use peritus_model_protocol::{
    JsonBounds, JsonSchema, ProtocolLimits, SchemaDialect, ToolArgumentSchema,
};
use serde_json::{Value, json};
fn compile(value: &Value) -> Result<ToolArgumentSchema, Box<dyn std::error::Error>> {
    Ok(ToolArgumentSchema::compile(&JsonSchema::parse(
        &value.to_string(),
        SchemaDialect::Draft202012,
        JsonBounds::schema(ProtocolLimits::PRODUCTION),
    )?)?)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let schema = compile(&json!({"type":"object","properties":{
        "evidence":{"type":"array","items":{"anyOf":[
            {"type":"object","properties":{"authority":{"const":"official"},"version":{"type":"string","minLength":1},"reference":{"$ref":"#/$defs/web"}},"required":["authority","version","reference"],"additionalProperties":false},
            {"type":"object","properties":{"authority":{"const":"engineering_rationale"},"version":{"type":["string","null"]},"reference":{"type":"object","properties":{"kind":{"const":"rationale"},"invariant":{"type":"string","minLength":1}},"required":["invariant"],"additionalProperties":false}},"required":["authority","version","reference"],"additionalProperties":false}
        ]}},"optional":{"type":"string"},"nullable":{"type":["string","null"]}},"required":["evidence","nullable"],"additionalProperties":false,
        "$defs":{"web":{"type":"object","properties":{"kind":{"const":"web_source"},"source":{"type":"string"}},"required":["kind","source"],"additionalProperties":false}}}))?;
    let valid = json!({"evidence":[{"authority":"official","version":"2026-11-07","reference":{"kind":"web_source","source":"retained"}}],"nullable":null});
    schema.validate_value(&valid)?;
    for (pointer, replacement, expected) in [
        ("/evidence/0/version", Value::Null, "/evidence/0/version"),
        ("/evidence/0/reference/kind", json!("rationale"), "/evidence/0/reference/kind"),
    ] {
        let mut invalid = valid.clone();
        *invalid.pointer_mut(pointer).ok_or("bad replay pointer")? = replacement;
        assert_eq!(schema.validate_value(&invalid).unwrap_err().path(), expected);
    }
    let mut provider_value = valid.clone();
    provider_value["optional"] = Value::Null;
    let projected = schema.provider_schema()?;
    compile(&projected)?.validate_value(&provider_value)?;
    schema.restore_provider_value(&mut provider_value)?;
    assert_eq!(provider_value, valid, "only omission markers are removed; actual null survives");
    let rational = json!({"evidence":[{"authority":"engineering_rationale","version":null,"reference":{"invariant":"Retain committed state"}}],"nullable":null});
    schema.validate_value(&rational)?;
    let integers = compile(
        &json!({"type":"integer","minimum":9_007_199_254_740_993_u64,"maximum":18_446_744_073_709_551_614_u64}),
    )?;
    for (value, expected) in [
        (9_007_199_254_740_992_u64, false),
        (9_007_199_254_740_993, true),
        (18_446_744_073_709_551_614, true),
        (u64::MAX, false),
    ] {
        assert_eq!(integers.validate_value(&json!(value)).is_ok(), expected);
    }
    let enumerated = compile(&json!({"enum":[1]}))?;
    enumerated.validate_value(&json!(1.0))?;
    let unique = compile(&json!({"type":"array","uniqueItems":true}))?;
    assert!(unique.validate_value(&json!([1, 1.0])).is_err());
    let unicode = compile(&json!({"type":"string","minLength":2,"maxLength":2}))?;
    unicode.validate_value(&json!("é🦜"))?;
    assert!(unicode.validate_value(&json!("é")).is_err());
    assert!(
        compile(&json!({"type":"string","pattern":".*"})).is_err(),
        "unsupported restrictions fail closed"
    );
    let mut cases = 0;
    for limit in -40..40 {
        let range = compile(&json!({"type":"number","minimum":limit,"exclusiveMaximum":limit+4}))?;
        for offset in -2..7 {
            let value = limit + offset;
            assert_eq!(
                range.validate_value(&json!(value)).is_ok(),
                (limit..limit + 4).contains(&value)
            );
            cases += 1;
        }
    }
    let optional_nullable = compile(
        &json!({"type":"object","properties":{"optional":{"type":["string","null"]}},"additionalProperties":false}),
    )?;
    assert!(
        optional_nullable.provider_schema().is_err(),
        "omission and explicit null must not be conflated"
    );
    let enumeration = compile(
        &json!({"type":"object","properties":{"authority":{"enum":["primary","official"]}},"required":["authority"],"additionalProperties":false}),
    )?;
    assert!(
        enumeration.provider_schema().is_ok(),
        "enum discriminators must remain provider-enforced"
    );
    let bounds = [-1e250, -1.01, -1e-20, 0.0, 1e-20, 1.0, 1e18, 1e250];
    let ranges = bounds
        .iter()
        .map(|b| compile(&json!({"type":"number","minimum":b})))
        .collect::<Result<Vec<_>, _>>()?;
    let mut seed = 0x5742_a95c_d716_38eb_u64;
    let mut floating_cases = 0;
    for _ in 0..1000 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let value = f64::from_bits(seed);
        if value.is_finite() {
            for (bound, schema) in bounds.iter().zip(&ranges) {
                assert_eq!(
                    schema.validate_value(&json!(value)).is_ok(),
                    value >= *bound,
                    "{value} >= {bound}"
                );
                floating_cases += 1;
            }
        }
    }
    println!("PASS: {floating_cases} generated finite-float comparisons; seed=5742a95cd71638eb");
    println!(
        "PASS: observed field failures, nullable projection, exact integer limits, numeric equality, Unicode and {cases} generated range cases"
    );
    Ok(())
}
