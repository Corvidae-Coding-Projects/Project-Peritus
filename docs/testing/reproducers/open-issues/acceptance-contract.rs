//! Real public-API replay for issue #155's Make-owned data workspace.

use std::{env, error::Error, fs, path::PathBuf, process::Command};

use peritus_gates::TargetGatePlan;

fn main() -> Result<(), Box<dyn Error>> {
    let root = PathBuf::from(env::args_os().nth(1).ok_or("fixture root is required")?);
    if root.exists() && fs::read_dir(&root)?.next().is_some() {
        return Err("fixture root must be absent or empty".into());
    }
    fs::create_dir_all(root.join("data/modpacks"))?;
    fs::write(
        root.join("Makefile"),
        concat!(
            ".PHONY: check check-fast\n",
            "check:\n",
            "\t@test \"$$(wc -c < data/modpacks/alpha.mrpack)\" -eq 4\n",
            "\t@grep -qx '{\"name\":\"alpha\"}' data/modpacks/alpha.json\n",
            "\t@printf 'FULL-CHECK-PASS\\n'\n",
            "check-fast:\n",
            "\t@printf 'FAST-CHECK-PASS\\n'\n",
        ),
    )?;
    fs::write(root.join("data/modpacks/alpha.mrpack"), [0_u8, 1, 2, 3])?;
    fs::write(root.join("data/modpacks/alpha.json"), "{\"name\":\"alpha\"}\n")?;

    let changed = ["Makefile", "data/modpacks/alpha.mrpack", "data/modpacks/alpha.json"]
        .into_iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    let plan = TargetGatePlan::discover(&root, changed.clone(), &[])?;
    if !plan.has_complete_coverage() || !plan.uncovered_paths().is_empty() {
        return Err(format!("candidate remained uncovered: {:?}", plan.uncovered_paths()).into());
    }
    let projects =
        plan.projects().iter().map(|project| format!("{:?}", project.kind())).collect::<Vec<_>>();
    if projects != ["Make"] {
        return Err(format!("expected one Make project, observed {projects:?}").into());
    }
    let make_commands =
        plan.commands().iter().filter(|command| command.program() == "make").collect::<Vec<_>>();
    if make_commands.len() != 1 || make_commands[0].arguments() != ["check"] {
        return Err(format!("expected only make check, observed {make_commands:?}").into());
    }
    for path in &changed {
        fs::read(root.join(path))?;
    }
    let command = make_commands[0];
    let output = Command::new(command.program())
        .args(command.arguments())
        .current_dir(root.join(command.current_dir()))
        .output()?;
    if !output.status.success() || output.stdout != b"FULL-CHECK-PASS\n" {
        return Err(format!(
            "selected check failed: status={:?} stdout={:?} stderr={:?}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        )
        .into());
    }
    println!(
        "PASS issue-155: projects={projects:?} command={} files={} output={}",
        command.display(),
        changed.len(),
        String::from_utf8_lossy(&output.stdout).trim(),
    );
    Ok(())
}
