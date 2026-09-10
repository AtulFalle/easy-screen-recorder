#[cfg(not(windows))]
mod stub;

#[cfg(windows)]
mod windows;

mod types;

pub use types::{CaptureDisplay, CaptureWindow, Recording};

#[cfg(windows)]
pub use windows::{list_displays, list_windows, probe, start};

#[cfg(not(windows))]
pub use stub::{list_displays, list_windows, probe, start};
