//! Receive pipe bytes only after their retention ownership has been reserved.

use tokio::io::{AsyncRead, AsyncReadExt as _};

use super::journal::{JournalSender, allocation_error};
use crate::ProviderCoreError;

const READ_BYTES: usize = 8 * 1024;

pub(super) async fn read_bounded(
    mut input: impl AsyncRead + Unpin,
    limit: usize,
    operation: &'static str,
    journal: Option<JournalSender>,
) -> Result<Vec<u8>, ProviderCoreError> {
    let mut output = Vec::new();
    let mut buffer = [0_u8; READ_BYTES];
    loop {
        // A read followed by an awaited send loses the received chunk if cancellation wins.
        // Reserve physical queue space and allocation first; sending after a read cannot suspend.
        let permit = match &journal {
            Some(sender) => Some(sender.reserve().await.map_err(|_| {
                ProviderCoreError::transport(
                    "process_journal",
                    "owned journal writer is unavailable",
                )
            })?),
            None => None,
        };
        let mut retained = Vec::new();
        if permit.is_some() {
            retained.try_reserve(READ_BYTES).map_err(|_| allocation_error())?;
        }
        output.try_reserve(READ_BYTES).map_err(|_| {
            ProviderCoreError::transport(
                "process_output",
                "owned subprocess output allocation failed",
            )
        })?;
        let count = input.read(&mut buffer).await.map_err(|_| {
            ProviderCoreError::transport("process_output", "owned subprocess output read failed")
        })?;
        if count == 0 {
            drop(permit);
            drop(journal);
            return Ok(output);
        }
        // Retain every received byte even when an explicit caller output ceiling is exceeded.
        if let Some(permit) = permit {
            retained.extend_from_slice(&buffer[..count]);
            permit.send(retained);
        }
        let next = output.len().checked_add(count).ok_or_else(|| {
            ProviderCoreError::limit_exceeded(
                "process_output",
                "subprocess output length overflowed",
            )
        })?;
        if next > limit {
            return Err(ProviderCoreError::limit_exceeded(
                operation,
                "owned subprocess output exceeded its byte limit",
            ));
        }
        output.extend_from_slice(&buffer[..count]);
    }
}
