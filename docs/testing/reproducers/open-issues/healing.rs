//! Actual public syntax-healing consumer with independently defined byte-preservation oracles.
use peritus_model_protocol::ProtocolLimits;
use peritus_provider_core::healing;
use serde_json::{Value, json};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut accepted = 0;
    let mut cases = 0;
    for code in 0_u8..32 {
        for suffix in ["ASCII", "café 🦀", "quoted \"value\" and literal \\n"] {
            let template = format!("before __CONTROL__ after {suffix}");
            let input = json!({"content":template,"tool_calls":[]})
                .to_string()
                .replace("__CONTROL__", &char::from(code).to_string());
            let expected = template.replace("__CONTROL__", &char::from(code).to_string());
            cases += 1;
            if let Ok(value) =
                healing::private_envelope(&input, "opencode.turn", ProtocolLimits::PRODUCTION)
            {
                let parsed: Value = serde_json::from_slice(value.value().canonical_bytes())?;
                assert_eq!(parsed["content"], expected);
                assert_eq!(parsed["tool_calls"], json!([]));
                accepted += 1;
            }
        }
    }
    let trailing = "{\"content\":\"Preserve this exact reply.\"},\"tool_calls\":[]}";
    let joined = healing::private_envelope(trailing, "opencode.turn", ProtocolLimits::PRODUCTION);
    if let Ok(value) = &joined {
        let parsed: Value = serde_json::from_slice(value.value().canonical_bytes())?;
        assert_eq!(parsed, json!({"content":"Preserve this exact reply.","tool_calls":[]}));
    }
    assert!(
        healing::object(trailing, "ordinary.tool.arguments", ProtocolLimits::PRODUCTION).is_err()
    );
    for invalid in [
        "{\"content\":\"first\",\"content\":\"second\"}",
        "{\"content\":\"first\",\"tool_calls\":[]},\"tool_calls\":[]}",
        "{\"content\":\"first\"},\"tool_calls\":[{\"name\":\"unexpected\"}]}",
        "{\"content\":\"first\"}{\"content\":\"second\"}",
        "{\"content\":\"unterminated",
    ] {
        assert!(
            healing::private_envelope(invalid, "opencode.turn", ProtocolLimits::PRODUCTION)
                .is_err()
        );
    }
    println!(
        "{}",
        json!({"control_character_cases":cases,"accepted_exact":accepted,"trailing_fragment_repaired":joined.is_ok(),"strict_boundary_cases":6})
    );
    if std::env::args().any(|arg| arg == "--require-fixed") {
        assert_eq!(accepted, cases);
        assert!(joined.is_ok());
    }
    Ok(())
}
