//! Peritus production daemon executable.

use peritus_daemon::{DaemonConfig, DaemonRuntime};

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let mut arguments = std::env::args_os();
    let executable = arguments.next().unwrap_or_default();
    let Some(command) = arguments.next() else {
        usage(&executable);
        std::process::exit(2);
    };
    let flag = arguments.next();
    let configuration = arguments.next();
    if command != "serve"
        || flag.as_deref() != Some(std::ffi::OsStr::new("--config"))
        || configuration.is_none()
        || arguments.next().is_some()
    {
        usage(&executable);
        std::process::exit(2);
    }
    let configuration = configuration.expect("checked configuration argument");
    let result = async {
        let config = DaemonConfig::load(configuration)?;
        let mut runtime = DaemonRuntime::start(config).await?;
        runtime.wait_for_shutdown_signal().await?;
        runtime.shutdown().await
    }
    .await;
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn usage(executable: &std::ffi::OsStr) {
    eprintln!("usage: {} serve --config <config.toml>", std::path::Path::new(executable).display(),);
}
