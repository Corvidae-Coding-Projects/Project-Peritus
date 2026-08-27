//! Error conversion for public constructors whose verified errors are Debug-only.

use std::fmt::Debug;
use std::io;

pub(super) fn debug_error(error: impl Debug) -> io::Error {
    io::Error::other(format!("{error:?}"))
}
