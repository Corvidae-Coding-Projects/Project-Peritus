//! An exited preview must settle on the first observation even after unattended output.
use super::*;
use peritus_process::ProcessCursor;
use peritus_types::RunId;
use std::{io::Write as _, thread, time::Instant};
