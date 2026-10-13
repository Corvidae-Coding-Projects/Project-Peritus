//! Actual consumer of the candidate plan discovered from a real checked-out workspace.
use peritus_gates::{TargetGatePlan, TargetGateReport};
use std::path::Path;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let normal = TargetGatePlan::discover(Path::new(&args[1]), vec![args[2].clone().into()], &[])?;
    let all = args.get(3).is_some_and(|mode| mode == "real-interface");
    let selected = if all { normal.clone().with_conventional_checks_unselected() } else { normal.clone().with_test_suites_unselected() };
    let mut suites = 0;
    for (before, after) in normal.commands().iter().zip(selected.commands()) {
        let rust_suite = before.program() == "cargo"
            && before.arguments().first().is_some_and(|verb| verb == "test");
        if rust_suite || (all && before.program() != "peritus-internal") {
            suites += 1;
            if before.optional_when_not_selected() || !after.optional_when_not_selected() {
                return Err("suite selection did not preserve the normal plan and explicit exclusion".into());
            }
        } else if before != after {
            return Err("test selection changed an unrelated check".into());
        }
        println!("{} | optional={} | {}", after.label(), after.optional_when_not_selected(), after.display());
    }
    if suites == 0 || TargetGateReport::from_execution(&selected, Vec::new()).passed() {
        return Err("missing execution evidence was accepted or no actual Rust suite was discovered".into());
    }
    println!("PASS: explicit selection retains suites as observations; missing required results cannot pass");
    Ok(())
}
