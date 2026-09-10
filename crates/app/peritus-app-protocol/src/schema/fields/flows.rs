//! Stateful application-protocol metadata grouped by stable concern.

use super::AppTypeDescriptor;

mod brief;
mod checkpoints;
mod compaction;
mod context;
mod daemon;
mod doctor;
mod files;
mod goal;
mod image_page;
mod images;
mod init;
mod inputs;
mod interaction;
mod launch;
mod library;
mod memory;
mod permissions;
mod product;
mod prompt_terminal;
mod review;
mod workbench;

/// Stateful flow metadata groups in dependency-before-consumer order.
pub const APP_FLOW_TYPES: &[&[AppTypeDescriptor]] = &[
    prompt_terminal::PROMPT_TERMINAL_TYPES,
    daemon::DAEMON_TYPES,
    doctor::DOCTOR_TYPES,
    review::REVIEW_TYPES,
    workbench::WORKBENCH_TYPES,
    launch::LAUNCH_TYPES,
    launch::RESULT_TYPES,
    permissions::PERMISSION_TYPES,
    init::INIT_TYPES,
    memory::MEMORY_TYPES,
    inputs::INPUT_TYPES,
    context::CONTEXT_TYPES,
    checkpoints::CHECKPOINT_TYPES,
    compaction::COMPACTION_TYPES,
    brief::BRIEF_TYPES,
    images::IMAGE_TYPES,
    image_page::IMAGE_PAGE_TYPES,
    files::FILE_TYPES,
    goal::GOAL_TYPES,
    library::LIBRARY_TYPES,
    product::PRODUCT_TYPES,
    product::SETTLEMENT_TYPES,
    interaction::INTERACTION_TYPES,
];
