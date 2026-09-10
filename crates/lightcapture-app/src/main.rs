#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![deny(unsafe_op_in_unsafe_fn)]

mod app;
mod pump;
mod settings;
mod status;
mod tray;

use std::process::ExitCode;

use single_instance::SingleInstance;

fn main() -> ExitCode {
    let Ok(instance) = SingleInstance::new("LightCapture-tray") else {
        eprintln!("could not create single-instance lock");
        return ExitCode::FAILURE;
    };
    if !instance.is_single() {
        eprintln!("LightCapture is already running");
        return ExitCode::SUCCESS;
    }
    match app::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}
