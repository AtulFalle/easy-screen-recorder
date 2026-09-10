/// Hardware encoder class, ordered by product preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EncoderKind {
    NvidiaNvenc = 0,
    IntelQuickSync = 1,
    AmdAmf = 2,
    HardwareOther = 3,
    Software = 4,
}

impl EncoderKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NvidiaNvenc => "NVIDIA NVENC",
            Self::IntelQuickSync => "Intel Quick Sync",
            Self::AmdAmf => "AMD AMF",
            Self::HardwareOther => "hardware H.264",
            Self::Software => "software H.264",
        }
    }
}

/// Snapshot of capture devices and the encoder Media Foundation would prefer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardwareInfo {
    pub displays: Vec<String>,
    pub encoder_names: Vec<String>,
    pub selected: EncoderKind,
}

impl HardwareInfo {
    #[must_use]
    pub fn software_fallback() -> Self {
        Self {
            displays: Vec::new(),
            encoder_names: Vec::new(),
            selected: EncoderKind::Software,
        }
    }
}

/// Classify a Media Foundation encoder friendly name.
#[must_use]
pub fn classify_encoder(name: &str) -> EncoderKind {
    let upper = name.to_ascii_uppercase();
    if upper.contains("NVIDIA") || upper.contains("NVENC") {
        EncoderKind::NvidiaNvenc
    } else if upper.contains("INTEL") || upper.contains("QSV") || upper.contains("QUICK SYNC") {
        EncoderKind::IntelQuickSync
    } else if upper.contains("AMD") || upper.contains("AMF") || upper.contains("VCE") {
        EncoderKind::AmdAmf
    } else {
        EncoderKind::HardwareOther
    }
}

/// Best encoder in NVIDIA → Intel → AMD → other hardware → software order.
#[must_use]
pub fn pick_encoder(names: &[String]) -> EncoderKind {
    names
        .iter()
        .map(|n| classify_encoder(n))
        .min()
        .unwrap_or(EncoderKind::Software)
}

/// Probe displays and (on Windows) report that MF will pick a hardware H.264 MFT at encode time.
pub fn probe() -> crate::Result<HardwareInfo> {
    crate::platform::probe()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_vendors() {
        assert_eq!(
            classify_encoder("NVIDIA H.264 Encoder MFT"),
            EncoderKind::NvidiaNvenc
        );
        assert_eq!(
            classify_encoder("Intel Quick Sync Video H.264 Encoder MFT"),
            EncoderKind::IntelQuickSync
        );
        assert_eq!(
            classify_encoder("AMD Hardware Encoder"),
            EncoderKind::AmdAmf
        );
    }

    #[test]
    fn pick_prefers_nvidia() {
        let names = vec![
            "Intel Quick Sync Video H.264 Encoder MFT".into(),
            "NVIDIA H.264 Encoder MFT".into(),
            "H264 Encoder".into(),
        ];
        assert_eq!(pick_encoder(&names), EncoderKind::NvidiaNvenc);
    }

    #[test]
    fn pick_empty_is_software() {
        assert_eq!(pick_encoder(&[]), EncoderKind::Software);
    }
}
