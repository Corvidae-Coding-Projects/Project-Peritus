//! Keep process interrupts owned by the UI while a foreground child shares its terminal.

use std::{
    io,
    task::{Context, Poll, Waker},
};

#[cfg(unix)]
pub(super) type Interrupts = tokio::signal::unix::Signal;
#[cfg(windows)]
pub(super) type Interrupts = tokio::signal::windows::CtrlC;

pub(super) fn listen() -> io::Result<Interrupts> {
    #[cfg(unix)]
    {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
    }
    #[cfg(windows)]
    {
        tokio::signal::windows::ctrl_c()
    }
}

pub(super) async fn next(interrupts: &mut Option<Interrupts>) {
    match interrupts {
        Some(interrupts) => {
            interrupts.recv().await;
        }
        None => std::future::pending().await,
    }
}

pub(super) fn drain(interrupts: &mut Interrupts) {
    let mut context = Context::from_waker(Waker::noop());
    while matches!(interrupts.poll_recv(&mut context), Poll::Ready(Some(()))) {}
}
