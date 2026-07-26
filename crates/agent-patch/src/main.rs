use agent_patch::app::run;
use agent_patch::cli::Cli;
use clap::Parser;
use std::io::{self, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let cli = Cli::parse();
    let config = cli.into_config();
    let output = run(config);

    if !output.stdout.is_empty() {
        let mut out = io::stdout().lock();
        let _ = writeln!(out, "{}", output.stdout);
    }
    if !output.stderr.is_empty() {
        let mut err = io::stderr().lock();
        let _ = writeln!(err, "{}", output.stderr);
    }

    ExitCode::from(output.exit_code)
}
