//! Reviewed entry points for native Debian and RPM package release tooling.

use std::{path::Path, process::Command};

use crate::XtaskError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Operation {
    Image,
    ImageSave,
    ImageRestore,
    Build,
    Compile,
    PackageCompiled,
    Sign,
    SignCi,
    Verify,
    Upload,
}

impl Operation {
    pub(crate) fn parse(name: &str) -> Option<Self> {
        match name {
            "distro-image" => Some(Self::Image),
            "distro-image-save" => Some(Self::ImageSave),
            "distro-image-restore" => Some(Self::ImageRestore),
            "distro-build" => Some(Self::Build),
            "distro-compile" => Some(Self::Compile),
            "distro-package-compiled" => Some(Self::PackageCompiled),
            "distro-sign" => Some(Self::Sign),
            "distro-sign-ci" => Some(Self::SignCi),
            "distro-verify" => Some(Self::Verify),
            "distro-upload" => Some(Self::Upload),
            _ => None,
        }
    }

    const fn argument(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::ImageSave => "image-save",
            Self::ImageRestore => "image-restore",
            Self::Build => "build",
            Self::Compile => "compile",
            Self::PackageCompiled => "package-compiled",
            Self::Sign => "sign",
            Self::SignCi => "sign-ci",
            Self::Verify => "verify",
            Self::Upload => "upload",
        }
    }
}

pub(crate) fn run(root: &Path, operation: Operation) -> Result<(), XtaskError> {
    let status = Command::new("python3")
        .current_dir(root)
        .arg(root.join("packaging/distro/main.py"))
        .arg(operation.argument())
        .status()
        .map_err(|error| XtaskError::io("start distribution package tooling from", root, error))?;
    if !status.success() {
        return Err(XtaskError::metadata(format!(
            "distribution package {} failed with {status}; inspect the retained package log",
            operation.argument()
        )));
    }
    Ok(())
}
