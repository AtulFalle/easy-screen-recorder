mod args;
mod run;

use std::process::ExitCode;

use clap::Parser;

use args::Cli;

fn main() -> ExitCode {
    match run::run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}
