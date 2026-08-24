//! Minimal native helper protocol used only by C4 integration tests.

use std::io::{Read, Write};

use peritus_process::{native_activation_record, native_ready_record};

pub fn exchange_and_exec() -> Result<(), ()> {
    let mut output = std::io::stdout().lock();
    output.write_all(native_ready_record().as_bytes()).map_err(|_| ())?;
    output.flush().map_err(|_| ())?;

    let mut input = std::io::stdin().lock();
    let mut length = [0_u8; 4];
    input.read_exact(&mut length).map_err(|_| ())?;
    let length = usize::try_from(u32::from_le_bytes(length)).map_err(|_| ())?;
    if !(32..=4 * 1_024 * 1_024).contains(&length) {
        return Err(());
    }
    let mut manifest = vec![0_u8; length];
    input.read_exact(&mut manifest).map_err(|_| ())?;
    let preparation = peritus_types::Sha256Digest::new(manifest[..32].try_into().map_err(|_| ())?);
    let digest = peritus_codec::sha256(&manifest);
    output.write_all(native_activation_record(digest, preparation).as_bytes()).map_err(|_| ())?;
    output.flush().map_err(|_| ())?;
    drop(output);
    drop(input);

    let mut arguments = std::env::args_os().skip(1);
    let executable = arguments.next().ok_or(())?;
    let mut command = std::process::Command::new(executable);
    command.args(arguments);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let _error = command.exec();
        Err(())
    }
    #[cfg(windows)]
    {
        let status = command.status().map_err(|_| ())?;
        std::process::exit(status.code().unwrap_or(125));
    }
}
