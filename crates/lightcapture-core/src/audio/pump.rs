use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::audio::mix::{
    f32_to_i16_le, mix_stereo, pcm_to_output_f32, CHUNK_FRAMES, OUTPUT_CHANNELS, OUTPUT_SAMPLE_RATE,
};
use crate::audio::queue::PcmQueue;
use crate::audio::wasapi::{ComInit, WasapiStream};
use crate::Error;

const MAX_PENDING_SAMPLES: usize = OUTPUT_SAMPLE_RATE as usize * OUTPUT_CHANNELS * 2;

pub(crate) struct AudioPump {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl AudioPump {
    pub(crate) fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.join.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for AudioPump {
    fn drop(&mut self) {
        self.stop();
    }
}

pub(crate) struct PumpStart {
    pub queue: Arc<PcmQueue>,
    pub pump: AudioPump,
}

enum Ready {
    Ok,
    Err(String),
}

pub(crate) fn start_pump(
    want_system: bool,
    want_mic: bool,
    paused: Arc<AtomicBool>,
) -> crate::Result<PumpStart> {
    let stop = Arc::new(AtomicBool::new(false));
    let queue = Arc::new(PcmQueue::new());
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let stop_thread = Arc::clone(&stop);
    let queue_thread = Arc::clone(&queue);
    let join = thread::Builder::new()
        .name("lightcapture-audio".into())
        .spawn(move || {
            run_pump(
                want_system,
                want_mic,
                paused,
                stop_thread,
                queue_thread,
                ready_tx,
            );
        })
        .map_err(|e| Error::Audio(e.to_string()))?;
    let ready = ready_rx
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| Error::Audio("audio pump did not start".into()))?;
    match ready {
        Ready::Ok => Ok(PumpStart {
            queue,
            pump: AudioPump {
                stop,
                join: Some(join),
            },
        }),
        Ready::Err(msg) => {
            stop.store(true, Ordering::SeqCst);
            let _ = join.join();
            Err(Error::Audio(msg))
        }
    }
}

fn run_pump(
    want_system: bool,
    want_mic: bool,
    paused: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    queue: Arc<PcmQueue>,
    ready_tx: mpsc::SyncSender<Ready>,
) {
    let _com = match ComInit::new() {
        Ok(com) => com,
        Err(err) => {
            let _ = ready_tx.send(Ready::Err(err.to_string()));
            return;
        }
    };
    let system = want_system
        .then(WasapiStream::loopback)
        .and_then(Result::ok);
    let mic = want_mic.then(WasapiStream::microphone).and_then(Result::ok);
    if system.is_none() && mic.is_none() {
        let _ = ready_tx.send(Ready::Err(
            "could not open system loopback or microphone".into(),
        ));
        return;
    }
    if ready_tx.send(Ready::Ok).is_err() {
        return;
    }

    let mut system_buf = Vec::new();
    let mut mic_buf = Vec::new();
    let drive_system = system.is_some();
    let chunk_samples = CHUNK_FRAMES * OUTPUT_CHANNELS;

    while !stop.load(Ordering::SeqCst) {
        if let Some(stream) = system.as_ref() {
            stream.wait();
        } else if let Some(stream) = mic.as_ref() {
            stream.wait();
        }

        if let Some(stream) = system.as_ref() {
            append_pcm(&mut system_buf, stream);
        }
        if let Some(stream) = mic.as_ref() {
            append_pcm(&mut mic_buf, stream);
        }
        cap_pending(&mut system_buf);
        cap_pending(&mut mic_buf);

        if paused.load(Ordering::Relaxed) {
            system_buf.clear();
            mic_buf.clear();
            continue;
        }

        while ready_chunk(drive_system, system_buf.len(), mic_buf.len(), chunk_samples) {
            let a = take_or_pad(&mut system_buf, chunk_samples);
            let b = take_or_pad(&mut mic_buf, chunk_samples);
            queue.push(f32_to_i16_le(&mix_stereo(&a, &b)));
        }
    }
    if !system_buf.is_empty() || !mic_buf.is_empty() {
        let a = take_or_pad(&mut system_buf, chunk_samples);
        let b = take_or_pad(&mut mic_buf, chunk_samples);
        queue.push(f32_to_i16_le(&mix_stereo(&a, &b)));
    }
}

fn append_pcm(buf: &mut Vec<f32>, stream: &WasapiStream) {
    if let Ok(bytes) = stream.read_pcm() {
        if !bytes.is_empty() {
            buf.extend(pcm_to_output_f32(&bytes, stream.format()));
        }
    }
}

fn ready_chunk(
    drive_system: bool,
    system_len: usize,
    mic_len: usize,
    chunk_samples: usize,
) -> bool {
    if drive_system {
        system_len >= chunk_samples
    } else {
        mic_len >= chunk_samples
    }
}

fn take_or_pad(buf: &mut Vec<f32>, samples: usize) -> Vec<f32> {
    let take = samples.min(buf.len());
    let mut out: Vec<f32> = buf.drain(..take).collect();
    out.resize(samples, 0.0);
    out
}

fn cap_pending(buf: &mut Vec<f32>) {
    if buf.len() > MAX_PENDING_SAMPLES {
        let extra = buf.len() - MAX_PENDING_SAMPLES;
        buf.drain(..extra);
    }
}
