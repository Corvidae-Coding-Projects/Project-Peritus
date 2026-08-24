//! Exact address connect and bounded bidirectional relay.

use std::{
    io::{Read, Write},
    net::{Shutdown, SocketAddr, TcpStream},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use crate::{
    ConnectionAccount, DestinationRequest, NetworkError, NetworkErrorKind, NetworkOperation,
    RecoveryClass, ResolvedDestination,
};

use super::owner::{SharedWorkerConfig, charge_total};

pub(super) fn open(
    config: &SharedWorkerConfig,
    request: &DestinationRequest,
) -> Result<(TcpStream, ResolvedDestination), NetworkError> {
    let mut resolved = config.resolver.resolve(&config.plan, request)?;
    resolved.sort_by_key(ResolvedDestination::address);
    let selected = resolved
        .into_iter()
        .next()
        .ok_or_else(|| connect_error("no admitted DNS answer is available"))?;
    let timeout =
        Duration::from_millis(config.plan.options().bounds().connection_millis().min(30_000));
    let stream =
        TcpStream::connect_timeout(&SocketAddr::new(selected.address(), request.port()), timeout)
            .map_err(|_| connect_error("admitted upstream connection failed"))?;
    stream.set_read_timeout(Some(Duration::from_millis(100))).map_err(|_| io_error())?;
    stream.set_write_timeout(Some(Duration::from_millis(100))).map_err(|_| io_error())?;
    Ok((stream, selected))
}

pub(super) fn copy_exact_bounded(
    reader: &mut impl Read,
    writer: &mut impl Write,
    bytes: u64,
    account: &mut ConnectionAccount,
    config: &SharedWorkerConfig,
    upload: bool,
    began: Instant,
) -> Result<(), NetworkError> {
    let mut remaining = bytes;
    let mut buffer = [0_u8; 8_192];
    while remaining > 0 {
        if config.cancellation.is_cancelled() {
            return Err(cancelled_error());
        }
        account.check_elapsed(elapsed(began))?;
        let wanted = usize::try_from(remaining.min(buffer.len() as u64)).unwrap_or(buffer.len());
        let read = match reader.read(&mut buffer[..wanted]) {
            Ok(0) => return Err(io_error()),
            Ok(read) => read,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                continue;
            }
            Err(_) => return Err(io_error()),
        };
        charge(account, config, u64::try_from(read).unwrap_or(u64::MAX), upload)?;
        writer.write_all(&buffer[..read]).map_err(|_| io_error())?;
        remaining = remaining.saturating_sub(u64::try_from(read).unwrap_or(u64::MAX));
    }
    Ok(())
}

pub(super) fn copy_to_eof_bounded(
    reader: &mut impl Read,
    writer: &mut impl Write,
    account: &mut ConnectionAccount,
    config: &SharedWorkerConfig,
    upload: bool,
    began: Instant,
) -> Result<(), NetworkError> {
    let mut buffer = [0_u8; 8_192];
    loop {
        if config.cancellation.is_cancelled() {
            return Err(cancelled_error());
        }
        account.check_elapsed(elapsed(began))?;
        match reader.read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(read) => {
                charge(account, config, u64::try_from(read).unwrap_or(u64::MAX), upload)?;
                writer.write_all(&buffer[..read]).map_err(|_| io_error())?;
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => return Err(io_error()),
        }
    }
}

pub(super) fn tunnel(
    client: TcpStream,
    upstream: TcpStream,
    account: &Arc<Mutex<ConnectionAccount>>,
    config: &SharedWorkerConfig,
) -> Result<(), NetworkError> {
    let mut client_read = client.try_clone().map_err(|_| io_error())?;
    let mut upstream_write = upstream.try_clone().map_err(|_| io_error())?;
    let upload_account = Arc::clone(account);
    let upload_config = config.clone();
    let upload = thread::Builder::new()
        .name("peritus-network-upload".to_owned())
        .spawn(move || {
            let result = copy_to_eof_shared(
                &mut client_read,
                &mut upstream_write,
                &upload_account,
                &upload_config,
                true,
            );
            let _ = upstream_write.shutdown(Shutdown::Write);
            result
        })
        .map_err(|_| io_error())?;
    let mut upstream_read = upstream;
    let mut client_write = client;
    let download =
        copy_to_eof_shared(&mut upstream_read, &mut client_write, account, config, false);
    let _ = client_write.shutdown(Shutdown::Write);
    let upload = upload.join().map_err(|_| io_error())?;
    upload.and(download)
}

fn copy_to_eof_shared(
    reader: &mut TcpStream,
    writer: &mut TcpStream,
    account: &Arc<Mutex<ConnectionAccount>>,
    config: &SharedWorkerConfig,
    upload: bool,
) -> Result<(), NetworkError> {
    let began = Instant::now();
    let mut buffer = [0_u8; 8_192];
    loop {
        if config.cancellation.is_cancelled() {
            return Err(cancelled_error());
        }
        account
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .check_elapsed(elapsed(began))?;
        match reader.read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(read) => {
                charge(
                    &mut account.lock().unwrap_or_else(std::sync::PoisonError::into_inner),
                    config,
                    u64::try_from(read).unwrap_or(u64::MAX),
                    upload,
                )?;
                writer.write_all(&buffer[..read]).map_err(|_| io_error())?;
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => return Err(io_error()),
        }
    }
}

fn charge(
    account: &mut ConnectionAccount,
    config: &SharedWorkerConfig,
    bytes: u64,
    upload: bool,
) -> Result<(), NetworkError> {
    if upload {
        account.charge_upload(bytes)?;
    } else {
        account.charge_download(bytes)?;
    }
    charge_total(&config.total_bytes, bytes, config.plan.options().bounds().total_bytes())
}

fn elapsed(began: Instant) -> u64 {
    u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX)
}

const fn connect_error(detail: &'static str) -> NetworkError {
    NetworkError::new(
        NetworkErrorKind::Connect,
        NetworkOperation::Connect,
        RecoveryClass::Retry,
        detail,
    )
}
const fn io_error() -> NetworkError {
    NetworkError::new(
        NetworkErrorKind::Io,
        NetworkOperation::Relay,
        RecoveryClass::CancelAndJoin,
        "managed proxy stream operation failed",
    )
}
const fn cancelled_error() -> NetworkError {
    NetworkError::new(
        NetworkErrorKind::IncompleteTeardown,
        NetworkOperation::Relay,
        RecoveryClass::CancelAndJoin,
        "managed proxy connection was cancelled",
    )
}
