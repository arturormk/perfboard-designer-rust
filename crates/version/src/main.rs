//! Prints the effective Perfboard Designer version.
//!
//! ```text
//! perfboard-version                 # 0.1.26
//! perfboard-version --tag           # v0.1.26
//! perfboard-version --check-tag T   # succeeds only if T names this release
//! ```

use std::process::ExitCode;

use perfboard_version::{Version, commit_count, resolve};

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let directory = std::env::current_dir().unwrap_or_else(|_| ".".into());
    let version = match resolve(env!("CARGO_PKG_VERSION"), commit_count(&directory)) {
        Ok(version) => version,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::FAILURE;
        }
    };
    match arguments
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] => println!("{version}"),
        ["--tag"] => println!("v{version}"),
        ["--check-tag", tag] => return check_tag(version, tag),
        _ => {
            eprintln!("usage: perfboard-version [--tag | --check-tag <tag>]");
            return ExitCode::from(2);
        }
    }
    ExitCode::SUCCESS
}

/// Release tags must name an even-minor version equal to the workspace one.
fn check_tag(version: Version, tag: &str) -> ExitCode {
    if !version.is_release_line() {
        eprintln!(
            "error: {version} is a development version (odd minor); \
             only even-minor versions are released"
        );
        return ExitCode::FAILURE;
    }
    if tag != format!("v{version}") {
        eprintln!("error: tag {tag} does not match the workspace version v{version}");
        return ExitCode::FAILURE;
    }
    println!("v{version}");
    ExitCode::SUCCESS
}
