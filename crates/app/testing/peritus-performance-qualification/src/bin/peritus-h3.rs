//! H3 production qualification operator entry point.

#[cfg(unix)]
fn main() {
    use std::error::Error as _;

    use peritus_benchmarks::QualificationVerdict;
    use peritus_performance_qualification::{OPERATOR_USAGE, OperatorError, OperatorOptions};

    let outcome =
        OperatorOptions::parse(std::env::args_os().skip(1)).and_then(OperatorOptions::execute);
    match outcome {
        Ok(published) => {
            println!("evidence: {}", published.root().display());
            println!("verdict: {:?}", published.report().verdict());
            if published.report().verdict() == QualificationVerdict::NotReady {
                std::process::exit(3);
            }
        }
        Err(OperatorError::HelpRequested) => print!("{OPERATOR_USAGE}"),
        Err(error @ OperatorError::Usage(_)) => {
            eprintln!("peritus-h3: {error}\n\n{OPERATOR_USAGE}");
            std::process::exit(2);
        }
        Err(error) => {
            eprintln!("peritus-h3: {error}");
            let mut source = error.source();
            while let Some(cause) = source {
                eprintln!("  caused by: {cause}");
                source = cause.source();
            }
            std::process::exit(1);
        }
    }
}

#[cfg(not(unix))]
fn main() {
    eprintln!("peritus-h3: integrated H3 qualification currently requires Linux or macOS");
    std::process::exit(2);
}
