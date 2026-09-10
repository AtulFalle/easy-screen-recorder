use crate::config::RecordConfig;
use crate::hardware::HardwareInfo;
use crate::platform::types::{CaptureDisplay, CaptureWindow, Recording};
use crate::Error;

pub fn probe() -> crate::Result<HardwareInfo> {
    Ok(HardwareInfo::software_fallback())
}

pub fn list_displays() -> crate::Result<Vec<CaptureDisplay>> {
    Err(Error::UnsupportedPlatform)
}

pub fn list_windows() -> crate::Result<Vec<CaptureWindow>> {
    Err(Error::UnsupportedPlatform)
}

pub fn start(_config: RecordConfig) -> crate::Result<Recording> {
    Err(Error::UnsupportedPlatform)
}
