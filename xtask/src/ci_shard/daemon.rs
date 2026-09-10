//! Exhaustive serial Windows daemon test partitions within the retained job deadline.

use super::Operation;

const PRODUCT: &str = "product_run::tests::";
const FOLDER: &str = "product_run::tests::folder::";
const WORKBENCH: &str = "product_run::tests::workbench::";
const MODELS: &str = "product_run::tests::model_selection::";
const LIBRARY: &str = "product_run::tests::workbench::library::";
const REVIEW: &str = "product_run::tests::workbench::review::";
const CHECKPOINTS: &str = "product_run::tests::workbench::checkpoints::";

const REWINDS: &str = "product_run::tests::workbench::checkpoints::combined_";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Partition {
    Product,
    Folder,
    Workbench,
    Checkpoints,
    Rewinds,
    Models,
    Library,
    Review,
}

pub(super) fn test_filters(operation: Operation, windows: bool) -> Vec<&'static str> {
    match operation {
        // Only Windows needs additional jobs. The other native suites remain complete here.
        Operation::TestDaemon if windows => vec!["--skip", PRODUCT],
        Operation::TestDaemonPartition(Partition::Product) => {
            vec![PRODUCT, "--skip", FOLDER, "--skip", WORKBENCH, "--skip", MODELS]
        }
        Operation::TestDaemonPartition(Partition::Folder) => vec![FOLDER],
        Operation::TestDaemonPartition(Partition::Workbench) => {
            vec![WORKBENCH, "--skip", CHECKPOINTS, "--skip", LIBRARY, "--skip", REVIEW]
        }
        Operation::TestDaemonPartition(Partition::Checkpoints) => {
            vec![CHECKPOINTS, "--skip", REWINDS]
        }
        Operation::TestDaemonPartition(Partition::Rewinds) => vec![REWINDS],
        Operation::TestDaemonPartition(Partition::Models) => vec![MODELS],
        Operation::TestDaemonPartition(Partition::Library) => vec![LIBRARY],
        Operation::TestDaemonPartition(Partition::Review) => vec![REVIEW],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests;
