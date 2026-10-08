//! Exact address connect and bounded bidirectional relay.

use std::{
    io::{self, Read, Write},
    net::{Shutdown, SocketAddr, TcpStream},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use mio::{Events, Interest, Poll, Token, net::TcpStream as MioTcpStream};

use crate::{
    ConnectionAccount, DestinationRequest, NetworkError, NetworkErrorKind, NetworkOperation,
    RecoveryClass, ResolvedDestination,
};

use super::owner::SharedWorkerConfig;

const CANCELLATION_POLL: Duration = Duration::from_millis(250);
const CONNECT_TOKEN: Token = Token(0);

pub(super) fn open(
    config: &SharedWorkerConfig,
    request: &DestinationRequest,
    began: Instant,
) -> Result<(TcpStream, ResolvedDestination), NetworkError> {
    let mut resolved = config.resolver.resolve(&config.plan, request)?;
    resolved.sort_by_key(ResolvedDestination::address);
    let selected = resolved
        .into_iter()
        .next()
        .ok_or_else(|| connect_error("no admitted DNS answer is available"))?;
    let selected_timeout = config
        .plan
        .options()
        .bounds()
        .connection_millis()
        .map(std::num::NonZeroU64::get);
    let address = SocketAddr::new(selected.address(), request.port());
    let stream = connect_cancellable(config, address, began, selected_timeout)?;
    stream.set_read_timeout(Some(CANCELLATION_POLL)).map_err(|_| io_error())?;
    stream.set_write_timeout(Some(CANCELLATION_POLL)).map_err(|_| io_error())?;
    Ok((stream, selected))
}

fn connect_cancellable(
    config: &SharedWorkerConfig,
    address: SocketAddr,
    began: Instant,
    selected_timeout: Option<u64>,
) -> Result<TcpStream, NetworkError> {
    if config.cancellation.is_cancelled() {
        return Err(connect_cancelled_error());
    }
    let _ = connect_poll_window(began, selected_timeout)?;

    let mut stream = MioTcpStream::connect(address)
        .map_err(|_| connect_error("admitted upstream connection failed"))?;
    let mut poll = Poll::new()
        .map_err(|_| connect_error("upstream connection readiness setup failed"))?;
    poll.registry()
        .register(&mut stream, CONNECT_TOKEN, Interest::WRITABLE)
        .map_err(|_| connect_error("upstream connection readiness setup failed"))?;
    let mut events = Events::with_capacity(1);

    loop {
        if config.cancellation.is_cancelled() {
            return Err(connect_cancelled_error());
        }
        let timeout = connect_poll_window(began, selected_timeout)?;
        match poll.poll(&mut events, Some(timeout)) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(connect_error("upstream connection readiness wait failed")),
        }
        if config.cancellation.is_cancelled() {
            return Err(connect_cancelled_error());
        }
        let _ = connect_poll_window(began, selected_timeout)?;
        if !events.iter().any(|event| event.token() == CONNECT_TOKEN) {
            continue;
        }
        match stream.take_error() {
            Ok(Some(_)) | Err(_) => {
                return Err(connect_error("admitted upstream connection failed"));
            }
            Ok(None) => {}
        }
        match stream.peer_addr() {
            Ok(_) => break,
            Err(error) if connect_is_pending(&error) => continue,
            Err(_) => return Err(connect_error("admitted upstream connection failed")),
        }
    }

    if config.cancellation.is_cancelled() {
        return Err(connect_cancelled_error());
    }
    poll.registry()
        .deregister(&mut stream)
        .map_err(|_| connect_error("upstream connection readiness cleanup failed"))?;
    let stream = TcpStream::from(stream);
    stream.set_nonblocking(false).map_err(|_| io_error())?;
    Ok(stream)
}

fn connect_poll_window(
    began: Instant,
    selected_timeout: Option<u64>,
) -> Result<Duration, NetworkError> {
    let Some(limit) = selected_timeout else {
        return Ok(CANCELLATION_POLL);
    };
    let remaining = limit
        .checked_sub(elapsed(began)?)
        .filter(|remaining| *remaining > 0)
        .ok_or_else(duration_error)?;
    Ok(CANCELLATION_POLL.min(Duration::from_millis(remaining)))
}

fn connect_is_pending(error: &io::Error) -> bool {
    if matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::NotConnected
    ) {
        return true;
    }
    #[cfg(unix)]
    {
        error.raw_os_error() == Some(nix::errno::Errno::EINPROGRESS as i32)
    }
    #[cfg(not(unix))]
    {
        false
    }
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
        account.check_elapsed(elapsed(began)?)?;
        let wanted = usize::try_from(remaining.min(buffer.len() as u64))
            .map_err(|_| accounting_error())?;
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
        let count = u64::try_from(read).map_err(|_| accounting_error())?;
        charge(account, config, count, upload)?;
        write_all_owned(writer, &buffer[..read], config, began)?;
        remaining = remaining.checked_sub(count).ok_or_else(accounting_error)?;
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
        account.check_elapsed(elapsed(began)?)?;
        match reader.read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(read) => {
                let count = u64::try_from(read).map_err(|_| accounting_error())?;
                charge(account, config, count, upload)?;
                write_all_owned(writer, &buffer[..read], config, began)?;
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
    began: Instant,
) -> Result<(), NetworkError> {
    let failure = Arc::new(TunnelFailure {
        error: Mutex::new(None),
        client: client.try_clone().map_err(|_| io_error())?,
        upstream: upstream.try_clone().map_err(|_| io_error())?,
    });
    let mut client_read = client.try_clone().map_err(|_| io_error())?;
    let mut upstream_write = upstream.try_clone().map_err(|_| io_error())?;
    let upload_account = Arc::clone(account);
    let upload_config = config.clone();
    let upload_failure = Arc::clone(&failure);
    let upload = thread::Builder::new()
        .name("peritus-network-upload".to_owned())
        .spawn(move || {
            match catch_unwind(AssertUnwindSafe(|| {
                copy_to_eof_shared(
                    &mut client_read,
                    &mut upstream_write,
                    &upload_account,
                    &upload_config,
                    true,
                    began,
                    &upload_failure,
                )
            })) {
                Ok(Ok(())) => match upstream_write.shutdown(Shutdown::Write) {
                    Ok(()) => Ok(()),
                    Err(_) => {
                        let error = io_error();
                        upload_failure.fail(error);
                        Err(error)
                    }
                },
                Ok(Err(error)) => {
                    upload_failure.fail(error);
                    Err(error)
                }
                Err(_) => {
                    let error = io_error();
                    upload_failure.fail(error);
                    Err(error)
                }
            }
        })
        .map_err(|_| io_error())?;
    let mut upstream_read = upstream;
    let mut client_write = client;
    let download = match catch_unwind(AssertUnwindSafe(|| {
        copy_to_eof_shared(
            &mut upstream_read,
            &mut client_write,
            account,
            config,
            false,
            began,
            &failure,
        )
    })) {
        Ok(Ok(())) => match client_write.shutdown(Shutdown::Write) {
            Ok(()) => Ok(()),
            Err(_) => {
                let error = io_error();
                failure.fail(error);
                Err(error)
            }
        },
        Ok(Err(error)) => {
            failure.fail(error);
            Err(error)
        }
        Err(_) => {
            let error = io_error();
            failure.fail(error);
            Err(error)
        }
    };
    let upload = match upload.join() {
        Ok(result) => result,
        Err(_) => {
            let error = io_error();
            failure.fail(error);
            Err(error)
        }
    };
    if let Some(error) = failure.get() {
        return Err(error);
    }
    upload.and(download)
}

struct TunnelFailure {
    error: Mutex<Option<NetworkError>>,
    client: TcpStream,
    upstream: TcpStream,
}

impl TunnelFailure {
    fn fail(&self, error: NetworkError) {
        let mut failure = self.error.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let first = failure.is_none();
        if first {
            *failure = Some(error);
        }
        drop(failure);
        if first {
            let _ = self.client.shutdown(Shutdown::Both);
            let _ = self.upstream.shutdown(Shutdown::Both);
        }
    }

    fn get(&self) -> Option<NetworkError> {
        *self.error.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

fn copy_to_eof_shared(
    reader: &mut TcpStream,
    writer: &mut TcpStream,
    account: &Arc<Mutex<ConnectionAccount>>,
    config: &SharedWorkerConfig,
    upload: bool,
    began: Instant,
    local_failure: &TunnelFailure,
) -> Result<(), NetworkError> {
    let mut buffer = [0_u8; 8_192];
    loop {
        if let Some(error) = local_failure.get() {
            return Err(error);
        }
        if config.cancellation.is_cancelled() {
            return Err(cancelled_error());
        }
        account
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .check_elapsed(elapsed(began)?)?;
        match reader.read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(read) => {
                charge(
                    &mut account.lock().unwrap_or_else(std::sync::PoisonError::into_inner),
                    config,
                    u64::try_from(read).map_err(|_| accounting_error())?,
                    upload,
                )?;
                write_all_tunnel(writer, &buffer[..read], config, began, local_failure)?;
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

fn write_all_tunnel(
    writer: &mut impl Write,
    mut bytes: &[u8],
    config: &SharedWorkerConfig,
    began: Instant,
    local_failure: &TunnelFailure,
) -> Result<(), NetworkError> {
    while !bytes.is_empty() {
        if let Some(error) = local_failure.get() {
            return Err(error);
        }
        if config.cancellation.is_cancelled() {
            return Err(cancelled_error());
        }
        if let Some(limit) = config.plan.options().bounds().connection_millis()
            && elapsed(began)? > limit.get()
        {
            return Err(duration_error());
        }
        match writer.write(bytes) {
            Ok(0) => return Err(io_error()),
            Ok(written) => bytes = &bytes[written..],
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => return Err(io_error()),
        }
    }
    Ok(())
}

fn charge(
    account: &mut ConnectionAccount,
    config: &SharedWorkerConfig,
    bytes: u64,
    upload: bool,
) -> Result<(), NetworkError> {
    super::owner::charge(
        account,
        &config.total_bytes,
        bytes,
        config.plan.options().bounds().total_bytes(),
        upload,
    )
}

pub(super) fn write_all_owned(
    writer: &mut impl Write,
    mut bytes: &[u8],
    config: &SharedWorkerConfig,
    began: Instant,
) -> Result<(), NetworkError> {
    while !bytes.is_empty() {
        if config.cancellation.is_cancelled() {
            return Err(cancelled_error());
        }
        if let Some(limit) = config.plan.options().bounds().connection_millis() {
            if elapsed(began)? > limit.get() {
                return Err(duration_error());
            }
        }
        match writer.write(bytes) {
            Ok(0) => return Err(io_error()),
            Ok(written) => bytes = &bytes[written..],
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => return Err(io_error()),
        }
    }
    Ok(())
}

fn elapsed(began: Instant) -> Result<u64, NetworkError> {
    u64::try_from(began.elapsed().as_millis()).map_err(|_| accounting_error())
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

const fn connect_cancelled_error() -> NetworkError {
    NetworkError::new(
        NetworkErrorKind::IncompleteTeardown,
        NetworkOperation::Connect,
        RecoveryClass::CancelAndJoin,
        "managed upstream connection was cancelled",
    )
}

const fn accounting_error() -> NetworkError {
    NetworkError::new(
        NetworkErrorKind::Limit,
        NetworkOperation::Relay,
        RecoveryClass::CancelAndJoin,
        "managed proxy byte or duration accounting is not representable",
    )
}

const fn duration_error() -> NetworkError {
    NetworkError::new(
        NetworkErrorKind::Limit,
        NetworkOperation::Connect,
        RecoveryClass::CancelAndJoin,
        "managed connection crossed its selected duration ceiling",
    )
}
