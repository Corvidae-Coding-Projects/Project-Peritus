//! Disposable native plugin used through the real host's framed transport.
use peritus_plugin_sdk::{
    HostRequest, PluginRequestEnvelope, PluginResponse, PluginResponseEnvelope, PluginStatus,
    decode_frame, encode_frame,
};
use std::io::{Read, Write};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::args().nth(1).ok_or("fixture mode required")?;
    std::fs::write("child.pid", std::process::id().to_string())?;
    if mode == "crash" || mode == "flood" {
        if mode == "flood" {
            std::io::stderr().write_all(&vec![b'x'; 2 * 1024 * 1024])?;
        }
        eprintln!("REAL-PLUGIN-FAILURE: intentional exit 23; café 🦀");
        std::process::exit(23);
    }
    if mode == "malformed" {
        eprintln!("REAL-PLUGIN-MALFORMED");
        std::io::stdout().write_all(&[0, 0, 0, 0])?;
        std::io::stdout().flush()?;
        std::thread::sleep(std::time::Duration::from_secs(30));
    }
    if mode == "timeout" {
        eprintln!("REAL-PLUGIN-TIMEOUT");
        std::thread::sleep(std::time::Duration::from_secs(30));
    }
    loop {
        let mut header = [0; 4];
        if std::io::stdin().read_exact(&mut header).is_err() {
            break;
        }
        let size = u32::from_be_bytes(header) as usize;
        let mut bytes = vec![0; size + 4];
        bytes[..4].copy_from_slice(&header);
        std::io::stdin().read_exact(&mut bytes[4..])?;
        let request: PluginRequestEnvelope = decode_frame(&bytes, 65536)?;
        let shutdown = matches!(request.request, HostRequest::Shutdown);
        let status = if shutdown { PluginStatus::Stopped } else { PluginStatus::Ready };
        let response = PluginResponseEnvelope {
            protocol_version: request.protocol_version,
            request_id: request.request_id,
            response: PluginResponse::Status { status },
        };
        std::io::stdout().write_all(&encode_frame(&response, 65536)?)?;
        std::io::stdout().flush()?;
        if shutdown {
            break;
        }
    }
    Ok(())
}
