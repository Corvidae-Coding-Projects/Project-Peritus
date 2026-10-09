//! Gateway startup, local listener, and graceful shutdown.

use crate::{api, config, state};
use std::sync::Arc;

#[tokio::main]
pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let options = config::Options::parse()?;
    let listener =
        tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, options.port)).await?;
    let address = listener.local_addr()?;
    let app = Arc::new(state::App::open(options, address.port())?);
    println!("Peritus console: http://{address}");
    println!("Configuration: {}", app.options.config_file.display());
    axum::serve(listener, api::router(app))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
