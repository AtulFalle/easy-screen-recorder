use lightcapture_core::{CaptureDisplay, SessionStats};
use tray_icon::Icon;

const ICON_SIZE: u32 = 32;

#[must_use]
pub fn tooltip(stats: Option<&SessionStats>, error: Option<&str>, hotkey_ok: bool) -> String {
    if let Some(err) = error {
        return format!("LightCapture — {err}");
    }
    let suffix = if hotkey_ok {
        String::new()
    } else {
        "  (hotkey unavailable)".into()
    };
    match stats {
        None => format!("LightCapture — idle{suffix}"),
        Some(s) => format!(
            "LightCapture — {}s  {}x{}  encoded {}  dropped {}{suffix}",
            s.elapsed_secs, s.width, s.height, s.frames_encoded, s.frames_dropped
        ),
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
        }
    }

    #[test]
    fn idle_tooltip() {
        assert_eq!(tooltip(None, None, true), "LightCapture — idle");
    }

    #[test]
    fn recording_tooltip() {
        let text = tooltip(Some(&stats()), None, true);
        assert!(text.contains("4s"));
        assert!(text.contains("1920x1080"));
        assert!(text.contains("encoded 8"));
        assert!(text.contains("dropped 2"));
    }

    #[test]
    fn error_tooltip_wins() {
        assert_eq!(
            tooltip(Some(&stats()), Some("encoder failed"), true),
            "LightCapture — encoder failed"
        );
    }

    #[test]
    fn hotkey_unavailable_noted_when_idle() {
        assert!(tooltip(None, None, false).contains("hotkey unavailable"));
    }

    #[test]
    fn empty_display_name_uses_index() {
        let display = CaptureDisplay {
            index: 1,
            name: String::new(),
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
            width: 2560,
            height: 1440,
        };
        assert_eq!(display_label(&display), "Office (2560x1440)");
    }
}
