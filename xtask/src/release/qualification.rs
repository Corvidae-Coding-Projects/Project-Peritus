//! Native qualification consumes the staged archive, never a replacement build.

use std::{fs, path::Path, process::Command};

use super::{QualificationInput, XtaskError, digest, run};

#[cfg(test)]
mod tests;

pub(super) fn prepare(root: &Path) -> Result<(), XtaskError> {
    let package = crate::product_package::package_path(root);
    let name = package.file_name().ok_or_else(|| XtaskError::metadata("package name missing"))?;
    crate::product_package::native_artifacts::restore_controllers(root)?;
    run(
        Command::new(if cfg!(windows) { "python" } else { "python3" })
            .current_dir(root)
            .arg(root.join("packaging/qualification.py"))
            .arg(root.join("target/release-input"))
            .arg(root)
            .arg(name),
        "restore checksum-verified release archive for native qualification",
    )?;
    crate::product_package::qualification::archive_prepared(root, QualificationInput::Release)?;
    Ok(())
}

pub(super) fn copy_archive(root: &Path, archive: &Path, checksum: &Path) -> Result<(), XtaskError> {
    let name = archive.file_name().ok_or_else(|| XtaskError::metadata("archive name missing"))?;
    let checksum_name = checksum
        .file_name()
        .ok_or_else(|| XtaskError::metadata("archive checksum name missing"))?;
    let source = root.join("dist").join(name);
    let source_checksum = root.join("dist").join(checksum_name);
    let expected = fs::read_to_string(&source_checksum).map_err(|error| {
        XtaskError::io("read staged release checksum at", &source_checksum, error)
    })?;
    if expected != format!("{}\n", digest(&source)?) {
        return Err(XtaskError::metadata("staged release archive checksum mismatch"));
    }
    fs::copy(&source, archive)
        .map_err(|error| XtaskError::io("copy exact staged release archive to", archive, error))?;
    fs::copy(&source_checksum, checksum).map_err(|error| {
        XtaskError::io("copy exact staged release checksum to", checksum, error)
    })?;
    Ok(())
}
