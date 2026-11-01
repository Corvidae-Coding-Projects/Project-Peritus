//! Public diagnostic constructors must bound valid Unicode without panicking.
use peritus_mcp::{BridgeError, BridgeErrorClass, JsonRpcResponse, McpError, McpErrorClass, RpcId};
use std::{error::Error, io};

fn expected_prefix(text: &str, limit: usize) -> String {
    let mut result = String::new();
    for scalar in text.chars() {
        if result.len() + scalar.len_utf8() > limit {
            break;
        }
        result.push(scalar);
    }
    result
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut failures = 0;
    for (name, bound) in [
        ("response", 512),
        ("bridge-code", 128),
        ("bridge-detail", 1024),
        ("transport-detail", 1024),
    ] {
        for symbol in ["é", "界", "🦀"] {
            let input = format!("{}{symbol}{symbol}", "a".repeat(bound - 1));
            let expected = expected_prefix(&input, bound);
            let actual = std::panic::catch_unwind(|| match name {
                "response" => {
                    JsonRpcResponse::failure(Some(RpcId::Number(7)), -32603, input.clone())
                        .error
                        .map(|value| value.message)
                        .unwrap_or_default()
                }
                "bridge-code" => {
                    BridgeError::new(BridgeErrorClass::Authorization, input.clone(), "detail")
                        .code()
                        .to_owned()
                }
                "bridge-detail" => {
                    BridgeError::new(BridgeErrorClass::Infrastructure, "code", input.clone())
                        .detail()
                        .to_owned()
                }
                _ => McpError::new(McpErrorClass::Protocol, "fixture", input.clone())
                    .detail()
                    .to_owned(),
            });
            if actual.as_ref().is_ok_and(|text| text == &expected) {
                println!("PASS {name}: {symbol:?}, bounded prefix length {}", expected.len());
            } else {
                eprintln!(
                    "FAIL {name}: valid Unicode crossing byte {bound} must return its complete-scalar prefix"
                );
                failures += 1;
            }
        }
    }
    // Short input and exact-bound ASCII must remain unchanged, including source metadata.
    let error = BridgeError::with_source(
        BridgeErrorClass::Infrastructure,
        "scope",
        "é",
        io::Error::other("cause"),
    );
    assert_eq!(error.detail(), "é");
    assert!(error.source().is_some());
    let message = "x".repeat(512);
    assert_eq!(
        JsonRpcResponse::failure(None, -32603, &message).error.map(|e| e.message),
        Some(message)
    );
    if failures != 0 {
        return Err(format!("{failures} public Unicode diagnostic failures").into());
    }
    Ok(())
}
