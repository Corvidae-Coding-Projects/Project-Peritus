//! Product command dispatch, separate from release staging and policy commands.

use super::{Command, Path, Write, XtaskError, qualification, write_output};

pub(super) fn execute_product(
    command: Command,
    root: &Path,
    output: &mut dyn Write,
) -> Result<(), XtaskError> {
    let (package, message) = match command {
        Command::ProductPackage => (crate::product_package::build(root)?, "product package ready"),
        Command::ProductInstall => {
            (crate::product_package::install(root)?, "product installed; start it with `peritus`")
        }
        Command::ProductPackageSmoke => {
            (crate::product_package::smoke(root)?, "native product lifecycle passed")
        }
        Command::ProductNativeQualification => {
            (qualification::qualify(root)?, "native H2 qualification passed; retained report")
        }
        Command::ProductNativeQualificationPrepare => {
            (qualification::prepare(root)?, "native H2 package prepared without rebuilding")
        }
        Command::ProductNativeQualificationRestore => {
            (qualification::restore(root)?, "same-run native H2 package restored")
        }
        Command::ProductNativeQualificationShard { index, input } => (
            qualification::qualify_shard(root, index, input)?,
            "native H2 qualification shard passed; retained reports",
        ),
        _ => return Err(XtaskError::invocation("command is not a product packaging operation")),
    };
    write_output(output, &format!("{message}: {}\n", package.display()))
}
