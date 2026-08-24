//! Unix descriptor bridge for a listener bound inside a fresh target network namespace.

use core::fmt;
use std::{
    fs::File,
    io::{IoSlice, IoSliceMut},
    net::TcpListener,
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd, RawFd},
        unix::net::UnixStream,
    },
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::Duration,
};

use nix::{
    errno::Errno,
    fcntl::{FcntlArg, FdFlag, fcntl},
    sys::socket::{ControlMessage, ControlMessageOwned, MsgFlags, cmsg_space, recvmsg, sendmsg},
};

use crate::{
    CancellationToken, NetworkError, NetworkObservation, NetworkPlan, ProxyCredential, Resolver,
    RoutingToken,
};

use super::{ProxyEndpoint, ProxyShutdown, owner};

const LISTENER_RECORD: &[u8] = b"peritus-netns-listener-v1";

/// Parent-owned managed proxy whose accepted listener is supplied from a child network namespace.
#[must_use = "the inherited-listener proxy must be shut down or dropped to join its owner"]
pub struct InheritedListenerProxy {
    child_channel: Option<File>,
    endpoint: Arc<Mutex<Option<ProxyEndpoint>>>,
    token: Arc<RoutingToken>,
    cancellation: CancellationToken,
    observations: Arc<Mutex<owner::ObservationLog>>,
    join: Option<JoinHandle<Result<ProxyShutdown, NetworkError>>>,
}

impl InheritedListenerProxy {
    pub(crate) fn start_with(
        plan: NetworkPlan,
        token: RoutingToken,
        resolver: Arc<dyn Resolver>,
        credential: Option<Arc<ProxyCredential>>,
    ) -> Result<Self, NetworkError> {
        let (parent_channel, child_channel) = UnixStream::pair()
            .map_err(|_| owner::proxy_error("netns proxy channel cannot be created"))?;
        parent_channel
            .set_nonblocking(true)
            .map_err(|_| owner::proxy_error("netns proxy channel cannot be made nonblocking"))?;
        let child_channel = File::from(OwnedFd::from(child_channel));
        let token = Arc::new(token);
        let cancellation = CancellationToken::new();
        let observations = Arc::new(Mutex::new(owner::ObservationLog::new(
            plan.options().bounds().observations(),
            plan.digest(),
        )));
        let endpoint = Arc::new(Mutex::new(None));
        let config = owner::OwnerConfig {
            plan: Arc::new(plan),
            token: Arc::clone(&token),
            resolver,
            credential,
            cancellation: cancellation.clone(),
            observations: Arc::clone(&observations),
        };
        let owner_endpoint = Arc::clone(&endpoint);
        let join = thread::Builder::new()
            .name("peritus-network-netns-proxy".to_owned())
            .spawn(move || run(&parent_channel, &owner_endpoint, config))
            .map_err(|_| owner::proxy_error("netns proxy owner thread cannot be started"))?;
        Ok(Self {
            child_channel: Some(child_channel),
            endpoint,
            token,
            cancellation,
            observations,
            join: Some(join),
        })
    }

    /// Transfers the child side of the protected listener-descriptor channel exactly once.
    ///
    /// # Errors
    ///
    /// Rejects a repeated transfer.
    pub fn take_listener_channel(&mut self) -> Result<File, NetworkError> {
        self.child_channel
            .take()
            .ok_or_else(|| owner::proxy_error("netns proxy child channel was already transferred"))
    }

    /// Returns the namespace-local loopback endpoint after the helper supplied its listener.
    #[must_use]
    pub fn endpoint(&self) -> Option<ProxyEndpoint> {
        *self.endpoint.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Borrows the opaque per-launch routing token for protected-handle staging.
    #[must_use]
    pub fn routing_token(&self) -> &RoutingToken {
        &self.token
    }

    /// Returns a snapshot of retained normalized observations.
    #[must_use]
    pub fn observations(&self) -> Vec<NetworkObservation> {
        self.observations.lock().unwrap_or_else(std::sync::PoisonError::into_inner).values.clone()
    }

    /// Cancels the listener and joins every accepted connection worker.
    ///
    /// # Errors
    ///
    /// Returns a typed failure when the channel, owner, or worker teardown is incomplete.
    pub fn shutdown(mut self) -> Result<ProxyShutdown, NetworkError> {
        self.join_owner()
    }

    fn join_owner(&mut self) -> Result<ProxyShutdown, NetworkError> {
        let _ = self.cancellation.cancel();
        self.child_channel.take();
        let join = self
            .join
            .take()
            .ok_or_else(|| owner::teardown_error("netns proxy owner was already joined"))?;
        let result = join
            .join()
            .map_err(|_| owner::teardown_error("netns proxy owner thread panicked"))??;
        if !result.workers_joined() {
            return Err(owner::teardown_error("netns proxy worker teardown was incomplete"));
        }
        Ok(result)
    }
}

impl fmt::Debug for InheritedListenerProxy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("InheritedListenerProxy")
            .field("endpoint", &self.endpoint())
            .field("routing_token", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

impl Drop for InheritedListenerProxy {
    fn drop(&mut self) {
        if self.join.is_some() {
            let _ = self.join_owner();
        }
    }
}

/// Passes one loopback listener from the native helper to its parent proxy owner.
///
/// `channel_handle` is the exact manifest-bound inherited Unix stream descriptor. The listener
/// remains bound to the helper's current network namespace even after the parent receives a
/// duplicate, allowing the isolated target to connect locally without receiving general egress.
///
/// # Errors
///
/// Rejects an invalid channel, non-loopback listener, zero port, or incomplete descriptor send.
#[allow(
    unsafe_code,
    reason = "BorrowedFd is the narrow SCM_RIGHTS adapter over a live manifest-bound descriptor"
)]
pub fn send_inherited_listener(
    channel_handle: u64,
    listener: &TcpListener,
) -> Result<ProxyEndpoint, NetworkError> {
    let channel = i32::try_from(channel_handle)
        .map_err(|_| owner::proxy_error("netns proxy channel descriptor is invalid"))?;
    let endpoint = ProxyEndpoint(
        listener
            .local_addr()
            .map_err(|_| owner::proxy_error("netns proxy listener address is unavailable"))?,
    );
    if !endpoint.socket_addr().ip().is_loopback() || endpoint.socket_addr().port() == 0 {
        return Err(owner::proxy_error("netns proxy listener is not nonzero loopback"));
    }
    // SAFETY: the helper calls this only with the live descriptor retained in its checked manifest;
    // the borrow lasts only for this send and cannot close or transfer channel ownership.
    let channel = unsafe { std::os::fd::BorrowedFd::borrow_raw(channel) };
    let payload = [IoSlice::new(LISTENER_RECORD)];
    let descriptors = [listener.as_raw_fd()];
    let control = [ControlMessage::ScmRights(&descriptors)];
    let sent = sendmsg::<()>(channel.as_raw_fd(), &payload, &control, MsgFlags::empty(), None)
        .map_err(|_| owner::proxy_error("netns proxy listener descriptor send failed"))?;
    if sent != LISTENER_RECORD.len() {
        return Err(owner::proxy_error("netns proxy listener descriptor send was incomplete"));
    }
    Ok(endpoint)
}

fn run(
    channel: &UnixStream,
    endpoint: &Arc<Mutex<Option<ProxyEndpoint>>>,
    config: owner::OwnerConfig,
) -> Result<ProxyShutdown, NetworkError> {
    let listener = receive_listener(channel, &config.cancellation)?;
    listener
        .set_nonblocking(true)
        .map_err(|_| owner::proxy_error("received netns listener cannot be nonblocking"))?;
    let observed = ProxyEndpoint(
        listener
            .local_addr()
            .map_err(|_| owner::proxy_error("received netns listener address is unavailable"))?,
    );
    if !observed.socket_addr().ip().is_loopback() || observed.socket_addr().port() == 0 {
        return Err(owner::proxy_error("received netns listener is not nonzero loopback"));
    }
    *endpoint.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(observed);
    owner::run(&listener, config)
}

#[allow(
    unsafe_code,
    reason = "SCM_RIGHTS returns a newly owned descriptor requiring one FromRawFd ownership claim"
)]
fn receive_listener(
    channel: &UnixStream,
    cancellation: &CancellationToken,
) -> Result<TcpListener, NetworkError> {
    while !cancellation.is_cancelled() {
        let mut bytes = [0_u8; LISTENER_RECORD.len()];
        let (message_bytes, message_flags, received) = {
            let mut slices = [IoSliceMut::new(&mut bytes)];
            let mut space = vec![0_u8; cmsg_space::<[RawFd; 1]>()];
            let message = match recvmsg::<()>(
                channel.as_raw_fd(),
                &mut slices,
                Some(&mut space),
                descriptor_receive_flags(),
            ) {
                Ok(message) => message,
                Err(Errno::EAGAIN) => {
                    thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(_) => return Err(owner::proxy_error("netns proxy listener receive failed")),
            };
            let mut received = Vec::new();
            for control in message
                .cmsgs()
                .map_err(|_| owner::proxy_error("netns proxy control record is invalid"))?
            {
                if let ControlMessageOwned::ScmRights(descriptors) = control {
                    received.extend(descriptors);
                }
            }
            (message.bytes, message.flags, received)
        };
        if message_bytes != LISTENER_RECORD.len()
            || bytes != LISTENER_RECORD
            || message_flags.intersects(MsgFlags::MSG_TRUNC | MsgFlags::MSG_CTRUNC)
        {
            for descriptor in received {
                let _ = nix::unistd::close(descriptor);
            }
            return Err(owner::proxy_error("netns proxy listener record is invalid"));
        }
        if received.len() != 1 {
            for descriptor in received {
                let _ = nix::unistd::close(descriptor);
            }
            return Err(owner::proxy_error("netns proxy descriptor count is invalid"));
        }
        let descriptor = received[0];
        // SAFETY: SCM_RIGHTS returned one fresh owned descriptor and this is its sole owner claim.
        let owned = unsafe { OwnedFd::from_raw_fd(descriptor) };
        let flags = fcntl(&owned, FcntlArg::F_GETFD)
            .map_err(|_| owner::proxy_error("netns proxy descriptor flags are unavailable"))?;
        let flags = FdFlag::from_bits_retain(flags) | FdFlag::FD_CLOEXEC;
        fcntl(&owned, FcntlArg::F_SETFD(flags))
            .map_err(|_| owner::proxy_error("netns proxy descriptor cannot be close-on-exec"))?;
        return Ok(TcpListener::from(owned));
    }
    Err(owner::teardown_error("netns proxy listener wait was cancelled"))
}

#[cfg(any(target_os = "android", target_os = "linux"))]
const fn descriptor_receive_flags() -> MsgFlags {
    MsgFlags::MSG_CMSG_CLOEXEC
}

#[cfg(not(any(target_os = "android", target_os = "linux")))]
const fn descriptor_receive_flags() -> MsgFlags {
    MsgFlags::empty()
}
