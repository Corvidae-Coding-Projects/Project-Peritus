//! Bounded newline-delimited MCP stdio framing.

use tokio::io::{AsyncBufRead, AsyncBufReadExt as _, AsyncWrite, AsyncWriteExt as _};

use crate::{JsonRpcResponse, McpError, McpErrorClass};

pub async fn read_message<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    maximum: usize,
) -> Result<Option<Vec<u8>>, McpError> {
    let mut output = Vec::new();
    loop {
        let available = reader.fill_buf().await.map_err(read_transport)?;
        if available.is_empty() {
            return if output.is_empty() {
                Ok(None)
            } else {
                Err(protocol("MCP input ended in a partial JSON message"))
            };
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let consumed = newline.map_or(available.len(), |index| index + 1);
        let payload_count = newline.unwrap_or(consumed);
        if output.len().saturating_add(payload_count) > maximum {
            return Err(McpError::new(
                McpErrorClass::Limit,
                "read MCP input",
                "MCP message exceeds its byte bound",
            ));
        }
        output.extend_from_slice(&available[..payload_count]);
        reader.consume(consumed);
        if newline.is_some() {
            if output.last() == Some(&b'\r') {
                output.pop();
            }
            if output.is_empty() {
                return Err(protocol("empty MCP messages are invalid"));
            }
            return Ok(Some(output));
        }
    }
}

pub async fn write_response<W: AsyncWrite + Unpin>(
    writer: &mut W,
    response: &JsonRpcResponse,
    maximum: usize,
) -> Result<(), McpError> {
    let payload = serde_json::to_vec(response).map_err(|error| {
        McpError::with_source(
            McpErrorClass::Protocol,
            "encode MCP response",
            error.to_string(),
            error,
        )
    })?;
    if payload.len() > maximum {
        return Err(McpError::new(
            McpErrorClass::Limit,
            "encode MCP response",
            "MCP response exceeds its byte bound",
        ));
    }
    writer.write_all(&payload).await.map_err(write_transport)?;
    writer.write_all(b"\n").await.map_err(delimiter_transport)?;
    writer.flush().await.map_err(flush_transport)
}

fn protocol(detail: &'static str) -> McpError {
    McpError::new(McpErrorClass::Protocol, "read MCP input", detail)
}

fn read_transport(error: std::io::Error) -> McpError {
    transport_error("read MCP input", error)
}

fn write_transport(error: std::io::Error) -> McpError {
    transport_error("write MCP response", error)
}

fn delimiter_transport(error: std::io::Error) -> McpError {
    transport_error("write MCP response delimiter", error)
}

fn flush_transport(error: std::io::Error) -> McpError {
    transport_error("flush MCP response", error)
}

fn transport_error(operation: &'static str, error: std::io::Error) -> McpError {
    McpError::with_source(McpErrorClass::Transport, operation, error.to_string(), error)
}
