//! WASAPI capture, mix, and bounded handoff into the encoder thread.

mod mix;
mod queue;

#[cfg(windows)]
mod pump;
#[cfg(windows)]
mod wasapi;

pub(crate) use mix::{missing_audio_frames, silence_i16_le, CHUNK_FRAMES};
pub(crate) use queue::PcmQueue;

#[cfg(windows)]
pub(crate) use pump::{start_pump, AudioPump};
