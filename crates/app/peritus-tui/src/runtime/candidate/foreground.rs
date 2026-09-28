//! Transfer terminal job control to the child and restore it before resuming the UI.

use std::{
    io,
    os::unix::process::CommandExt as _,
    process::{Command, ExitStatus},
};

use nix::{
    errno::Errno,
    sys::signal::SigSet,
    unistd::{Pid, tcgetpgrp, tcsetpgrp},
};

pub(super) fn status(command: &mut Command) -> io::Result<ExitStatus> {
    let input = io::stdin();
    let previous = match tcgetpgrp(&input) {
        Ok(group) => Some(group),
        Err(Errno::ENOTTY) => None,
        Err(error) => return Err(error.into()),
    };
    command.process_group(0);
    if previous.is_some() {
        configure_child_handoff(command);
    }
    let result = command.spawn().and_then(|mut child| child.wait());
    let restored = previous.map_or(Ok(()), |group| foreground(&input, group));
    restored?;
    result
}

fn foreground(input: &io::Stdin, group: Pid) -> io::Result<()> {
    // The waiting UI is now a background process group. Block SIGTTOU on this thread
    // while restoring ownership, then restore the exact previous thread signal mask.
    let mut blocked = SigSet::empty();
    blocked.add(nix::sys::signal::Signal::SIGTTOU);
    let previous = blocked.thread_swap_mask(nix::sys::signal::SigmaskHow::SIG_BLOCK)?;
    let result = tcsetpgrp(input, group);
    let restored = previous.thread_set_mask();
    result?;
    restored?;
    Ok(())
}

#[allow(
    unsafe_code,
    reason = "terminal foreground ownership must transfer in the child before any user code executes"
)]
fn configure_child_handoff(command: &mut Command) {
    // SAFETY: this fork-child-only closure uses async-signal-safe signal-mask and terminal
    // operations on inherited stdin. No allocation, locking, or parent memory mutation occurs.
    // Command::process_group establishes the child's group before pre_exec callbacks. Restoring
    // the exact signal mask prevents the executed program from inheriting a blocked SIGTTOU.
    unsafe {
        command.pre_exec(|| {
            let mut blocked = std::mem::zeroed::<libc::sigset_t>();
            let mut previous = std::mem::zeroed::<libc::sigset_t>();
            libc::sigemptyset(&raw mut blocked);
            libc::sigaddset(&raw mut blocked, libc::SIGTTOU);
            if libc::sigprocmask(libc::SIG_BLOCK, &raw const blocked, &raw mut previous) != 0 {
                return Err(io::Error::last_os_error());
            }
            let result = libc::tcsetpgrp(libc::STDIN_FILENO, libc::getpgrp());
            let error = (result != 0).then(io::Error::last_os_error);
            let restored =
                libc::sigprocmask(libc::SIG_SETMASK, &raw const previous, std::ptr::null_mut());
            if let Some(error) = error {
                return Err(error);
            }
            if restored != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
}
