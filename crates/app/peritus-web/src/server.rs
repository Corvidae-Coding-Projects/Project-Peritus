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
    let weak = Arc::downgrade(&app);
    let reaper = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let Some(app) = weak.upgrade() else {
                break;
            };
            match tokio::task::spawn_blocking(move || crate::processes::reap(&app)).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => eprintln!("peritus web: command reaping failed: {error}"),
                Err(error) => eprintln!("peritus web: command reaper task failed: {error}"),
            }
        }
    });
    let shutdown_app = Arc::clone(&app);
    let served = axum::serve(listener, api::router(app))
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            let commands = shutdown_app
                .processes
                .lock()
                .map(|processes| processes.values().cloned().collect::<Vec<_>>());
            if let Ok(commands) = commands {
                for command in commands {
                    if let Err(error) = command.cancel() {
                        eprintln!("peritus web: command shutdown requires recovery: {error}");
                    }
                }
            }
        })
        .await;
    reaper.abort();
    let _ = reaper.await;
    served?;
    Ok(())
}
