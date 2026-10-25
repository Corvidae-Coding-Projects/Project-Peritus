//! Point-of-use explanations of existing conversation authority.

use peritus_app_protocol::ProductInteractionMode as Mode;

pub const fn description(mode: Mode) -> &'static str {
    match mode {
        Mode::Chat => {
            "Discuss and inspect; explicitly authorized changes go through the build pipeline, within workspace permissions."
        }
        Mode::Plan => {
            "Read-only inspection and planning; cannot edit files or run commands. Switch explicitly to /chat or /build to request changes."
        }
        Mode::Design => {
            "Shape an idea through a guided conversation. Revise the saved draft in plain language; /design show displays it and /design build starts implementation."
        }
        Mode::Review => {
            "Independent read-only findings; cannot edit files or run commands. Switch explicitly to /chat or /build to request fixes."
        }
        Mode::Build => {
            "Run the design, writer, checks, reviewer and fixer delivery pipeline, within workspace permissions."
        }
    }
}

pub fn composer_title(mode: Mode, direct_folder: Option<bool>, active: bool, width: u16) -> String {
    let hint = if direct_folder == Some(false) {
        "read-only folder"
    } else {
        match mode {
            Mode::Plan | Mode::Review if active => "read-only · steer inspection",
            Mode::Plan | Mode::Review if direct_folder == Some(true) && width < 60 => {
                "read-only · /chat"
            }
            Mode::Plan | Mode::Review if direct_folder == Some(true) => {
                "read-only · /chat to implement"
            }
            Mode::Plan | Mode::Review if width < 60 => "read-only · /build",
            Mode::Plan | Mode::Review => "read-only · /build to implement",
            Mode::Chat | Mode::Build if active => "steer active work",
            Mode::Design if active => "shaping your design",
            Mode::Design => "answer or revise · /design show",
            Mode::Chat => "authorized work via pipeline",
            Mode::Build => "delivery pipeline",
        }
    };
    let name = match mode {
        Mode::Chat => "Chat",
        Mode::Plan => "Plan",
        Mode::Design => "Design",
        Mode::Review => "Review",
        Mode::Build => "Build",
    };
    format!(" {name} · {hint} ")
}
