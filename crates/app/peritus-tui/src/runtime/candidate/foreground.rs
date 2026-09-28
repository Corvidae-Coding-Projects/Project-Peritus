//! Transfer terminal job control to the child and restore it before resuming the UI.

use std::{
    io,
    os::unix::process::{CommandExt as _, ExitStatusExt as _},
    process::{Child, Command, ExitStatus},
};

use nix::{
    errno::Errno,
    sys::signal::{SigSet, Signal, killpg},
    unistd::{Pid, getpgrp, getsid, tcgetpgrp, tcsetpgrp},
};
use signal_hook::iterator::Signals;

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
    let mut resumes = Signals::new([libc::SIGCONT])?;
    let result = command.spawn().and_then(|mut child| {
        if previous.is_some() {
            wait_foreground(&mut child, &input, &mut resumes)
        } else {
            child.wait()
        }
    });
    let restored = previous.map_or(Ok(()), |group| foreground(&input, group));
    restored?;
    result
}

fn wait_foreground(
    child: &mut Child,
    input: &io::Stdin,
    resumes: &mut Signals,
) -> io::Result<ExitStatus> {
    let group = Pid::from_raw(i32::try_from(child.id()).map_err(io::Error::other)?);
    let result = loop {
        match wait_change(group) {
            Ok(status) if status.stopped_signal().is_some() => {
                if let Err(error) = suspend_and_resume(input, group, resumes) {
                    break Err(error);
                }
            }
            result => break result,
        }
    };
    if result.is_err() {
        // A failed handoff must not abandon a stopped command and its descendants.
        let _ = killpg(group, Signal::SIGKILL);
        let _ = child.wait();
    }
    result
}

fn suspend_and_resume(input: &io::Stdin, child: Pid, resumes: &mut Signals) -> io::Result<()> {
    let parent = getpgrp();
    foreground(input, parent)?;
    if parent != getsid(None)? {
        // Sending a stop can return before another thread actually stops the process. Wait
        // for the shell's SIGCONT receipt before transferring ownership or resuming the child.
        stop_until_continued(parent, Signal::SIGTSTP, resumes)?;
        while tcgetpgrp(input)? != parent {
            stop_until_continued(parent, Signal::SIGTTIN, resumes)?;
        }
    }
    // A session-leader group has no controlling shell job to suspend; continue its candidate.
    // Keep SIGTTOU unblocked here: the kernel must check ownership at the handoff itself.
    // A background continuation must stop rather than race the preceding ownership query.
    tcsetpgrp(input, child)?;
    killpg(child, Signal::SIGCONT)?;
    Ok(())
}

fn stop_until_continued(group: Pid, signal: Signal, resumes: &mut Signals) -> io::Result<()> {
    let _ = resumes.pending().count();
    killpg(group, signal)?;
    resumes.forever().next().ok_or_else(|| io::Error::other("resume signal listener closed"))?;
    Ok(())
}

#[allow(unsafe_code, reason = "retain the OS wait status while observing foreground job stops")]
fn wait_change(child: Pid) -> io::Result<ExitStatus> {
    loop {
        let mut status = 0;
        // SAFETY: child is the positive PID owned by this sole waiter; status is a valid
        // writable integer. WUNTRACED observes stops as well as exits without any pointers
        // into shared Rust state. ExitStatus receives the original platform status word.
        let result = unsafe { libc::waitpid(child.as_raw(), &raw mut status, libc::WUNTRACED) };
        if result >= 0 {
            return Ok(ExitStatus::from_raw(status));
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}

fn foreground(input: &io::Stdin, group: Pid) -> io::Result<()> {
    // The waiting UI is now a background process group. Block SIGTTOU on this thread
    // while restoring ownership, then restore the exact previous thread signal mask.
    let mut blocked = SigSet::empty();
    blocked.add(Signal::SIGTTOU);
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
