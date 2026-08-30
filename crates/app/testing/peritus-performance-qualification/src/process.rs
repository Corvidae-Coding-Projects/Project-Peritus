//! Owned local process effects for terminal and cancellation workloads.

use std::io::Read;
use std::process::{Child, ChildStdout, Command, Stdio};

use crate::SubjectError;

pub struct OwnedProcess {
    child: Child,
    stdout: ChildStdout,
    terminated: bool,
}

impl OwnedProcess {
    pub fn start() -> Result<Self, SubjectError> {
        let mut child = Command::new("yes")
            .arg("peritus-h3")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| std::io::Error::other("qualification process exposed no stdout"))?;
        Ok(Self { child, stdout, terminated: false })
    }

    pub fn read_exact(&mut self, bytes: usize) -> Result<(), SubjectError> {
        let mut remaining = bytes;
        let mut buffer = [0_u8; 16 * 1024];
        while remaining != 0 {
            let count = remaining.min(buffer.len());
            self.stdout.read_exact(&mut buffer[..count])?;
            remaining -= count;
        }
        Ok(())
    }

    pub fn terminate(&mut self) -> Result<(), SubjectError> {
        if self.terminated {
            return Ok(());
        }
        if self.child.try_wait()?.is_none() {
            self.child.kill()?;
        }
        self.child.wait()?;
        self.terminated = true;
        Ok(())
    }
}
