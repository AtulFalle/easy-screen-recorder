use lightcapture_core::{CaptureDisplay, SessionStats};
use tray_icon::Icon;
use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows::Win32::System::SystemInformation::GetSystemInfo;
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

const ICON_SIZE: u32 = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HotkeyStatus {
    pub record: bool,
    pub pause: bool,
}

impl HotkeyStatus {
    #[cfg(test)]
    #[must_use]
    pub const fn all_ok() -> Self {
        Self {
            record: true,
            pause: true,
        }
    }
}

#[must_use]
pub fn pause_menu_text(paused: bool) -> &'static str {
    if paused {
        "Resume recording"
    } else {
        "Pause recording"
    }
}

#[must_use]
pub fn tooltip(
    stats: Option<&SessionStats>,
    error: Option<&str>,
    paused: bool,
    hotkeys: HotkeyStatus,
) -> String {
    if let Some(err) = error {
        return truncate_tooltip(&format!("LightCapture — {err}"));
    }
    let suffix = hotkey_suffix(hotkeys);
    match stats {
        None => format!("LightCapture — idle{suffix}"),
        Some(s) => {
            let pause = if paused { " paused" } else { "" };
            let load = process_load();
            truncate_tooltip(&format!(
                "LightCapture — {enc} {fps}fps {width}x{height}{pause} {secs}s e{encoded} d{dropped} {cpu}% {ram}MB{suffix}",
                enc = s.encoder.as_short(),
                fps = s.fps_target,
                width = s.width,
                height = s.height,
                secs = s.elapsed_secs,
                encoded = s.frames_encoded,
                dropped = s.frames_dropped,
                cpu = load.cpu_pct,
                ram = load.rss_mb,
            ))
        }
    }
}

fn truncate_tooltip(text: &str) -> String {
    const MAX: usize = 127;
    if text.chars().count() <= MAX {
        return text.to_string();
    }
    text.chars().take(MAX.saturating_sub(1)).collect::<String>() + "…"
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProcessLoad {
    cpu_pct: u32,
    rss_mb: u32,
}

fn process_load() -> ProcessLoad {
    ProcessLoad {
        cpu_pct: process_cpu_pct(),
        rss_mb: process_rss_mb(),
    }
}

fn process_rss_mb() -> u32 {
    let mut counters = PROCESS_MEMORY_COUNTERS {
        cb: u32::try_from(std::mem::size_of::<PROCESS_MEMORY_COUNTERS>()).unwrap_or(0),
        ..Default::default()
    };
    // SAFETY: counters is a valid PROCESS_MEMORY_COUNTERS; GetCurrentProcess is this process.
    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) };
    if !ok.as_bool() {
        return 0;
    }
    u32::try_from(counters.WorkingSetSize / (1024 * 1024)).unwrap_or(0)
}

fn process_cpu_pct() -> u32 {
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    // SAFETY: all FILETIME slots are valid stack out-params for this process.
    let ok = unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
    };
    if ok.is_err() {
        return 0;
    }
    let used = filetime_100ns(kernel).saturating_add(filetime_100ns(user));
    static LAST: std::sync::Mutex<Option<(u64, std::time::Instant)>> = std::sync::Mutex::new(None);
    let Ok(mut last) = LAST.lock() else {
        return 0;
    };
    let now = std::time::Instant::now();
    let Some((prev_used, prev_at)) = *last else {
        *last = Some((used, now));
        return 0;
    };
    *last = Some((used, now));
    let elapsed = now.saturating_duration_since(prev_at);
    let elapsed_100ns = u64::try_from(elapsed.as_nanos() / 100).unwrap_or(0);
    if elapsed_100ns == 0 {
        return 0;
    }
    let cores = logical_cores().max(1);
    let delta = used.saturating_sub(prev_used);
    u32::try_from((delta.saturating_mul(100) / elapsed_100ns) / u64::from(cores)).unwrap_or(0)
}

fn filetime_100ns(time: FILETIME) -> u64 {
    (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
}

fn logical_cores() -> u32 {
    let mut info = windows::Win32::System::SystemInformation::SYSTEM_INFO::default();
    // SAFETY: SYSTEM_INFO is a valid out-param.
    unsafe { GetSystemInfo(&mut info) };
    info.dwNumberOfProcessors.max(1)
}

fn hotkey_suffix(hotkeys: HotkeyStatus) -> String {
    match (hotkeys.record, hotkeys.pause) {
        (true, true) => String::new(),
        (false, true) => "  (Ctrl+Shift+R unavailable)".into(),
        (true, false) => "  (Ctrl+Shift+P unavailable)".into(),
        (false, false) => "  (hotkeys unavailable)".into(),
    }
}

#[must_use]
pub fn display_label(display: &CaptureDisplay) -> String {
    let name = display.name.trim();
    if name.is_empty() {
        format!(
            "Display {} ({}x{})",
            display.index, display.width, display.height
        )
    } else {
        format!("{name} ({}x{})", display.width, display.height)
    }
}

#[must_use]
pub fn idle_icon() -> Icon {
    circle_icon(90, 96, 110)
}

#[must_use]
pub fn recording_icon() -> Icon {
    circle_icon(196, 48, 48)
}

#[must_use]
pub fn paused_icon() -> Icon {
    circle_icon(196, 140, 32)
}

fn circle_icon(red: u8, green: u8, blue: u8) -> Icon {
    let size = ICON_SIZE as usize;
    let mut rgba = vec![0_u8; size * size * 4];
    let center = 15_i32;
    let radius_sq = 12_i32 * 12_i32;
    for y in 0..ICON_SIZE as i32 {
        for x in 0..ICON_SIZE as i32 {
            let dx = x - center;
            let dy = y - center;
            if dx * dx + dy * dy <= radius_sq {
                let i =
                    (usize::try_from(y).unwrap_or(0) * size + usize::try_from(x).unwrap_or(0)) * 4;
                rgba[i] = red;
                rgba[i + 1] = green;
                rgba[i + 2] = blue;
                rgba[i + 3] = 255;
            }
        }
    }
    Icon::from_rgba(rgba, ICON_SIZE, ICON_SIZE).unwrap_or_else(|_| fallback_icon(red, green, blue))
}

fn fallback_icon(red: u8, green: u8, blue: u8) -> Icon {
    Icon::from_rgba(vec![red, green, blue, 255], 1, 1)
        .expect("1x1 RGBA icon is always a valid tray icon")
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightcapture_core::SessionStats;

    fn stats() -> SessionStats {
        SessionStats {
            frames_captured: 10,
            frames_encoded: 8,
            frames_dropped: 2,
            width: 1920,
            height: 1080,
            fps_target: 30,
            elapsed_secs: 4,
            audio_frames_sent: 0,
            audio_drops: 0,
            encoder: lightcapture_core::EncoderKind::NvidiaNvenc,
            failure: None,
        }
    }

    #[test]
    fn idle_tooltip() {
        assert_eq!(
            tooltip(None, None, false, HotkeyStatus::all_ok()),
            "LightCapture — idle"
        );
    }

    #[test]
    fn recording_tooltip() {
        let text = tooltip(Some(&stats()), None, false, HotkeyStatus::all_ok());
        assert!(text.contains("4s"));
        assert!(text.contains("1920x1080"));
        assert!(text.contains("e8"));
        assert!(text.contains("d2"));
        assert!(text.contains("NVENC"));
        assert!(text.contains("30fps"));
        assert!(!text.contains("paused"));
    }

    #[test]
    fn paused_tooltip() {
        let text = tooltip(Some(&stats()), None, true, HotkeyStatus::all_ok());
        assert!(text.contains("paused"));
        assert!(text.contains("4s"));
    }

    #[test]
    fn pause_menu_labels() {
        assert_eq!(pause_menu_text(false), "Pause recording");
        assert_eq!(pause_menu_text(true), "Resume recording");
    }

    #[test]
    fn error_tooltip_wins() {
        assert_eq!(
            tooltip(
                Some(&stats()),
                Some("encoder failed"),
                true,
                HotkeyStatus::all_ok()
            ),
            "LightCapture — encoder failed"
        );
    }

    #[test]
    fn hotkey_unavailable_noted_when_idle() {
        assert!(tooltip(
            None,
            None,
            false,
            HotkeyStatus {
                record: false,
                pause: true
            }
        )
        .contains("Ctrl+Shift+R unavailable"));
    }

    #[test]
    fn pause_hotkey_unavailable_noted_when_recording() {
        assert!(tooltip(
            Some(&stats()),
            None,
            false,
            HotkeyStatus {
                record: true,
                pause: false
            }
        )
        .contains("Ctrl+Shift+P unavailable"));
    }

    #[test]
    fn empty_display_name_uses_index() {
        let display = CaptureDisplay {
            index: 1,
            name: String::new(),
            device_id: r"\\.\DISPLAY1".into(),
            width: 1920,
            height: 1080,
        };
        assert_eq!(display_label(&display), "Display 1 (1920x1080)");
    }

    #[test]
    fn named_display_keeps_title() {
        let display = CaptureDisplay {
            index: 2,
            name: "Office".into(),
            device_id: r"\\.\DISPLAY2".into(),
            width: 2560,
            height: 1440,
        };
        assert_eq!(display_label(&display), "Office (2560x1440)");
    }
}
