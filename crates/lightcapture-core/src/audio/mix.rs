//! Convert WASAPI PCM to encoder stereo i16 @ 48 kHz and mix sources.

use std::time::Duration;

pub const OUTPUT_SAMPLE_RATE: u32 = 48_000;
pub const OUTPUT_CHANNELS: usize = 2;
/// 20 ms at 48 kHz.
pub const CHUNK_FRAMES: usize = 960;
/// Media Foundation / QPC tick: 100 nanoseconds.
pub const HNS_PER_SECOND: u64 = 10_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SampleFormat {
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
    pub float: bool,
}

impl SampleFormat {
    #[must_use]
    pub fn block_align(self) -> usize {
        usize::from(self.channels) * usize::from(self.bits_per_sample.max(8) / 8)
    }
}

/// Interleaved native PCM → interleaved stereo f32 at [`OUTPUT_SAMPLE_RATE`].
#[must_use]
pub fn pcm_to_output_f32(bytes: &[u8], format: SampleFormat) -> Vec<f32> {
    let native = pcm_to_f32(bytes, format);
    let stereo = to_stereo(&native, format.channels);
    resample_stereo(&stereo, format.sample_rate, OUTPUT_SAMPLE_RATE)
}

#[must_use]
pub fn pcm_to_f32(bytes: &[u8], format: SampleFormat) -> Vec<f32> {
    let channels = usize::from(format.channels.max(1));
    let width = usize::from(format.bits_per_sample.max(8) / 8);
    let frame_bytes = channels.saturating_mul(width);
    if frame_bytes == 0 {
        return Vec::new();
    }
    let frames = bytes.len() / frame_bytes;
    let mut out = Vec::with_capacity(frames * channels);
    for frame in 0..frames {
        let base = frame * frame_bytes;
        for ch in 0..channels {
            let off = base + ch * width;
            let sample = match (format.float, width) {
                (true, 4) => f32_from_le(&bytes[off..off + 4]),
                (false, 2) => {
                    let v = i16::from_le_bytes([bytes[off], bytes[off + 1]]);
                    f32::from(v) / 32768.0
                }
                (false, 4) => {
                    let v = i32::from_le_bytes([
                        bytes[off],
                        bytes[off + 1],
                        bytes[off + 2],
                        bytes[off + 3],
                    ]);
                    v as f32 / 2_147_483_648.0
                }
                _ => 0.0,
            };
            out.push(sample);
        }
    }
    out
}

#[must_use]
pub fn to_stereo(samples: &[f32], channels: u16) -> Vec<f32> {
    let channels = usize::from(channels.max(1));
    if channels == OUTPUT_CHANNELS {
        return samples.to_vec();
    }
    let frames = samples.len() / channels;
    let mut out = Vec::with_capacity(frames * OUTPUT_CHANNELS);
    for frame in 0..frames {
        let base = frame * channels;
        let left = samples[base];
        let right = if channels == 1 {
            left
        } else {
            samples[base + 1]
        };
        out.push(left);
        out.push(right);
    }
    out
}

#[must_use]
pub fn resample_stereo(input: &[f32], in_rate: u32, out_rate: u32) -> Vec<f32> {
    let in_rate = in_rate.max(1);
    let out_rate = out_rate.max(1);
    if in_rate == out_rate || input.len() < OUTPUT_CHANNELS {
        return input.to_vec();
    }
    let in_frames = input.len() / OUTPUT_CHANNELS;
    if in_frames == 0 {
        return Vec::new();
    }
    let out_frames =
        ((u64::from(in_frames as u32) * u64::from(out_rate)) / u64::from(in_rate)).max(1) as usize;
    let mut out = Vec::with_capacity(out_frames * OUTPUT_CHANNELS);
    for i in 0..out_frames {
        let src = i as f64 * f64::from(in_rate) / f64::from(out_rate);
        let i0 = src.floor() as usize;
        let frac = src - i0 as f64;
        let i1 = (i0 + 1).min(in_frames - 1);
        for ch in 0..OUTPUT_CHANNELS {
            let a = f64::from(input[i0 * OUTPUT_CHANNELS + ch]);
            let b = f64::from(input[i1 * OUTPUT_CHANNELS + ch]);
            out.push((a + (b - a) * frac) as f32);
        }
    }
    out
}

/// Sum two interleaved stereo buffers (pad the shorter with silence) and clamp.
#[must_use]
pub fn mix_stereo(a: &[f32], b: &[f32]) -> Vec<f32> {
    let n = a.len().max(b.len());
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let left = a.get(i).copied().unwrap_or(0.0);
        let right = b.get(i).copied().unwrap_or(0.0);
        out.push((left + right).clamp(-1.0, 1.0));
    }
    out
}

/// Emit a mix tick when *any* open source has a full chunk.
///
/// Loopback often delivers nothing while the render graph is silent. Waiting
/// on system audio in that case starved the microphone and left the MP4 with a
/// zero-duration AAC track that Windows players refuse.
#[must_use]
pub fn should_emit_mix_chunk(
    system_open: bool,
    mic_open: bool,
    system_len: usize,
    mic_len: usize,
    chunk_samples: usize,
) -> bool {
    (system_open && system_len >= chunk_samples) || (mic_open && mic_len >= chunk_samples)
}

/// After a WASAPI wait with no full chunk, still emit silence so the AAC
/// timeline covers video. Movies & TV rejects files whose audio duration is 0.
#[must_use]
pub fn should_emit_silence_tick(paused: bool, since_last_emit: Duration) -> bool {
    !paused && since_last_emit >= Duration::from_millis(20)
}

/// PCM frames (not samples) that a video timespan of `video_hns` 100ns units needs.
#[must_use]
pub fn pcm_frames_for_video_hns(video_hns: u64) -> u64 {
    video_hns.saturating_mul(u64::from(OUTPUT_SAMPLE_RATE)) / HNS_PER_SECOND
}

/// How many extra PCM frames to send so audio covers `video_hns`.
#[must_use]
pub fn missing_audio_frames(sent_frames: u64, video_hns: u64) -> u64 {
    pcm_frames_for_video_hns(video_hns).saturating_sub(sent_frames)
}

/// Interleaved stereo i16 silence for `frames` PCM frames.
#[must_use]
pub fn silence_i16_le(frames: usize) -> Vec<u8> {
    vec![0; frames.saturating_mul(OUTPUT_CHANNELS).saturating_mul(2)]
}

#[must_use]
pub fn f32_to_i16_le(samples: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        let v = (sample.clamp(-1.0, 1.0) * 32767.0).round() as i32;
        let v = v.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

fn f32_from_le(bytes: &[u8]) -> f32 {
    let mut buf = [0_u8; 4];
    buf.copy_from_slice(bytes);
    f32::from_le_bytes(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mono_i16_to_stereo() {
        let pcm = 32767_i16.to_le_bytes();
        let format = SampleFormat {
            sample_rate: OUTPUT_SAMPLE_RATE,
            channels: 1,
            bits_per_sample: 16,
            float: false,
        };
        let samples = pcm_to_output_f32(&pcm, format);
        assert_eq!(samples.len(), 2);
        assert!(samples[0] > 0.99);
        assert!((samples[0] - samples[1]).abs() < f32::EPSILON);
    }

    #[test]
    fn mix_clips() {
        let mixed = mix_stereo(&[0.8, 0.8], &[0.8, -0.8]);
        assert_eq!(mixed, vec![1.0, 0.0]);
    }

    #[test]
    fn mix_silence_and_signal() {
        let mixed = mix_stereo(&[0.0, 0.0], &[0.5, -0.25]);
        assert_eq!(mixed, vec![0.5, -0.25]);
    }

    #[test]
    fn resample_24k_doubles_frames() {
        let input = vec![0.0, 0.0, 1.0, 1.0];
        let out = resample_stereo(&input, 24_000, 48_000);
        assert_eq!(out.len() / OUTPUT_CHANNELS, 4);
    }

    #[test]
    fn i16_roundtrip_peak() {
        let bytes = f32_to_i16_le(&[1.0, -1.0]);
        assert_eq!(&bytes[..2], &32767_i16.to_le_bytes());
        assert_eq!(&bytes[2..], &(-32767_i16).to_le_bytes());
    }

    #[test]
    fn silent_loopback_does_not_block_microphone() {
        let chunk = CHUNK_FRAMES * OUTPUT_CHANNELS;
        assert!(should_emit_mix_chunk(true, true, 0, chunk, chunk));
    }

    #[test]
    fn silent_mic_does_not_block_loopback() {
        let chunk = CHUNK_FRAMES * OUTPUT_CHANNELS;
        assert!(should_emit_mix_chunk(true, true, chunk, 0, chunk));
    }

    #[test]
    fn closed_sources_do_not_emit() {
        let chunk = CHUNK_FRAMES * OUTPUT_CHANNELS;
        assert!(!should_emit_mix_chunk(false, false, chunk, chunk, chunk));
    }

    #[test]
    fn silence_tick_after_20ms_when_idle() {
        assert!(should_emit_silence_tick(false, Duration::from_millis(20)));
        assert!(!should_emit_silence_tick(false, Duration::from_millis(19)));
        assert!(!should_emit_silence_tick(true, Duration::from_millis(50)));
    }

    #[test]
    fn missing_audio_covers_one_second_of_video() {
        assert_eq!(pcm_frames_for_video_hns(HNS_PER_SECOND), 48_000);
        assert_eq!(missing_audio_frames(960, HNS_PER_SECOND), 48_000 - 960);
        assert_eq!(missing_audio_frames(48_000, HNS_PER_SECOND), 0);
        assert_eq!(silence_i16_le(2).len(), 8);
    }
}
