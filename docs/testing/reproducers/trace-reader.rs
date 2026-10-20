//! Operate the public reader on an actual append-only file, including a writer's torn tail.
use peritus_product_runner::{TraceReader, read_frames};
use std::{
    error::Error,
    fs::{self, OpenOptions},
    io::Write as _,
    path::PathBuf,
};

fn frame(kind: u8, text: &[u8]) -> Vec<u8> {
    let mut result = vec![kind];
    result.extend_from_slice(&(text.len() as u64).to_le_bytes());
    result.extend_from_slice(text);
    result
}

fn main() -> Result<(), Box<dyn Error>> {
    let path = PathBuf::from(std::env::args_os().nth(1).ok_or("disposable path required")?);
    let first = frame(2, b"first");
    let second = frame(4, b"second");
    fs::write(&path, &first)?;
    let mut snapshot = TraceReader::open(&path)?;
    OpenOptions::new().append(true).open(&path)?.write_all(&second)?;
    assert_eq!(snapshot.next_matching(|_| true)?.ok_or("first missing")?.payload(), b"first");
    assert!(snapshot.next_matching(|_| true)?.is_none());
    assert_eq!(snapshot.records_read(), 1);
    snapshot.rewind()?;
    snapshot.next_matching(|_| false)?;
    assert_eq!(snapshot.records_read(), 1);
    let collected = read_frames(&path)?;
    assert_eq!(collected.len(), 2);
    let mut fresh = TraceReader::open(&path)?;
    assert_eq!(fresh.next_matching(|_| true)?.as_ref(), collected.first());
    assert_eq!(fresh.next_matching(|_| true)?.as_ref(), collected.get(1));
    assert!(fresh.next_matching(|_| true)?.is_none());

    let mut prefix = first;
    prefix.extend_from_slice(&second[..11]);
    fs::write(&path, prefix)?;
    let mut torn = TraceReader::open(&path)?;
    OpenOptions::new().append(true).open(&path)?.write_all(&second[11..])?;
    assert_eq!(torn.next_matching(|_| true)?.ok_or("first missing")?.payload(), b"first");
    assert!(torn.next_matching(|_| true)?.is_none());
    assert_eq!(read_frames(&path)?.len(), 2);
    fs::remove_file(path)?;
    println!("PASS: snapshot append, rewind, compatibility, fresh reopen, completed torn tail");
    Ok(())
}
