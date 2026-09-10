use std::ptr;
use std::time::Duration;

use windows::core::GUID;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Media::Audio::{
    eCapture, eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDevice, IMMDeviceEnumerator,
    MMDeviceEnumerator, AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED,
    AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_LOOPBACK, WAVEFORMATEX,
    WAVEFORMATEXTENSIBLE,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
    COINIT_MULTITHREADED,
};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};

use crate::audio::mix::SampleFormat;
use crate::Error;

const WAVE_FORMAT_PCM: u16 = 1;
const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;
const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;
const SUBTYPE_PCM: GUID = GUID::from_u128(0x0000_0001_0000_0010_8000_00aa_0038_9b71);
const SUBTYPE_IEEE_FLOAT: GUID = GUID::from_u128(0x0000_0003_0000_0010_8000_00aa_0038_9b71);

pub(crate) struct ComInit {
    uninit: bool,
}

impl ComInit {
    pub(crate) fn new() -> crate::Result<Self> {
        // SAFETY: first COM init on this thread; failure codes are handled below.
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        Ok(Self { uninit: hr.is_ok() })
    }
}

impl Drop for ComInit {
    fn drop(&mut self) {
        if self.uninit {
            // SAFETY: paired with a successful CoInitializeEx on this thread.
            unsafe { CoUninitialize() };
        }
    }
}

pub(crate) struct WasapiStream {
    client: IAudioClient,
    capture: IAudioCaptureClient,
    event: Option<HANDLE>,
    format: SampleFormat,
    /// Kept alive until the client is stopped; Initialize may retain this pointer.
    _mix: MixFormat,
}

impl WasapiStream {
    pub(crate) fn loopback() -> crate::Result<Self> {
        open_endpoint(true)
    }

    pub(crate) fn microphone() -> crate::Result<Self> {
        open_endpoint(false)
    }

    #[must_use]
    pub(crate) fn format(&self) -> SampleFormat {
        self.format
    }

    pub(crate) fn wait(&self) {
        if let Some(event) = self.event {
            // SAFETY: `event` is a live auto-reset event owned by this stream.
            let _ = unsafe { WaitForSingleObject(event, 20) };
        } else {
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    pub(crate) fn read_pcm(&self) -> crate::Result<Vec<u8>> {
        let mut out = Vec::new();
        loop {
            let packet_frames = unsafe { self.capture.GetNextPacketSize() }
                .map_err(|e| Error::Audio(e.to_string()))?;
            if packet_frames == 0 {
                break;
            }
            let mut data = ptr::null_mut();
            let mut frames = 0_u32;
            let mut flags = 0_u32;
            unsafe {
                self.capture
                    .GetBuffer(&mut data, &mut frames, &mut flags, None, None)
                    .map_err(|e| Error::Audio(e.to_string()))?;
            }
            let copy = copy_packet(data, frames, flags, self.format, &mut out);
            // SAFETY: GetBuffer succeeded; ReleaseBuffer must run once for `frames`.
            let released = unsafe { self.capture.ReleaseBuffer(frames) };
            copy?;
            released.map_err(|e| Error::Audio(e.to_string()))?;
        }
        Ok(out)
    }
}

impl Drop for WasapiStream {
    fn drop(&mut self) {
        let _ = unsafe { self.client.Stop() };
        if let Some(event) = self.event.take() {
            let _ = unsafe { CloseHandle(event) };
        }
    }
}

fn open_endpoint(loopback: bool) -> crate::Result<WasapiStream> {
    let device = default_device(if loopback { eRender } else { eCapture })?;
    match open_on_device(&device, loopback, false) {
        Ok(stream) => Ok(stream),
        Err(_) => open_on_device(&device, loopback, true),
    }
}

fn default_device(flow: windows::Win32::Media::Audio::EDataFlow) -> crate::Result<IMMDevice> {
    let enumerator: IMMDeviceEnumerator = unsafe {
        CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
            .map_err(|e| Error::Audio(e.to_string()))?
    };
    unsafe {
        enumerator
            .GetDefaultAudioEndpoint(flow, eConsole)
            .map_err(|e| Error::Audio(e.to_string()))
    }
}

fn open_on_device(
    device: &IMMDevice,
    loopback: bool,
    event_driven: bool,
) -> crate::Result<WasapiStream> {
    let client: IAudioClient = unsafe {
        device
            .Activate(CLSCTX_ALL, None)
            .map_err(|e| Error::Audio(e.to_string()))?
    };
    let mix = MixFormat::from_client(&client)?;
    let format = parse_mix_format(mix.0)?;
    let mut flags = if loopback {
        AUDCLNT_STREAMFLAGS_LOOPBACK
    } else {
        0
    };
    let event = if event_driven {
        flags |= AUDCLNT_STREAMFLAGS_EVENTCALLBACK;
        Some(unsafe {
            CreateEventW(None, false, false, None).map_err(|e| Error::Audio(e.to_string()))?
        })
    } else {
        None
    };
    const BUFFER_HNS: i64 = 10_000_000;
    let init =
        unsafe { client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, BUFFER_HNS, 0, mix.0, None) };
    if let Err(err) = init {
        if let Some(handle) = event {
            let _ = unsafe { CloseHandle(handle) };
        }
        return Err(Error::Audio(err.to_string()));
    }
    if let Some(handle) = event {
        if let Err(err) = unsafe { client.SetEventHandle(handle) } {
            let _ = unsafe { CloseHandle(handle) };
            return Err(Error::Audio(err.to_string()));
        }
    }
    let capture: IAudioCaptureClient = unsafe {
        client
            .GetService()
            .map_err(|e| Error::Audio(e.to_string()))?
    };
    unsafe {
        client.Start().map_err(|e| Error::Audio(e.to_string()))?;
    }
    std::thread::sleep(Duration::from_millis(40));
    Ok(WasapiStream {
        client,
        capture,
        event,
        format,
        _mix: mix,
    })
}

struct MixFormat(*mut WAVEFORMATEX);

impl MixFormat {
    fn from_client(client: &IAudioClient) -> crate::Result<Self> {
        let ptr = unsafe { client.GetMixFormat() }.map_err(|e| Error::Audio(e.to_string()))?;
        if ptr.is_null() {
            return Err(Error::Audio("mix format was null".into()));
        }
        Ok(Self(ptr))
    }
}

impl Drop for MixFormat {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: pointer came from IAudioClient::GetMixFormat.
            unsafe { CoTaskMemFree(Some(self.0.cast())) };
            self.0 = ptr::null_mut();
        }
    }
}

fn parse_mix_format(ptr: *const WAVEFORMATEX) -> crate::Result<SampleFormat> {
    if ptr.is_null() {
        return Err(Error::Audio("mix format was null".into()));
    }
    // SAFETY: `ptr` is a valid WAVEFORMATEX from GetMixFormat for the lifetime of MixFormat.
    let tag = unsafe { ptr::addr_of!((*ptr).wFormatTag).read_unaligned() };
    let channels = unsafe { ptr::addr_of!((*ptr).nChannels).read_unaligned() };
    let sample_rate = unsafe { ptr::addr_of!((*ptr).nSamplesPerSec).read_unaligned() };
    let bits = unsafe { ptr::addr_of!((*ptr).wBitsPerSample).read_unaligned() };
    let cb_size = unsafe { ptr::addr_of!((*ptr).cbSize).read_unaligned() };
    let mut float = tag == WAVE_FORMAT_IEEE_FLOAT;
    if tag == WAVE_FORMAT_EXTENSIBLE && cb_size >= 22 {
        let ext = ptr.cast::<WAVEFORMATEXTENSIBLE>();
        // SAFETY: cbSize advertises the extensible tail.
        let sub = unsafe { ptr::addr_of!((*ext).SubFormat).read_unaligned() };
        float = sub == SUBTYPE_IEEE_FLOAT;
        if !float && sub != SUBTYPE_PCM {
            return Err(Error::Audio("unsupported extensible mix format".into()));
        }
    } else if tag != WAVE_FORMAT_PCM && tag != WAVE_FORMAT_IEEE_FLOAT {
        return Err(Error::Audio(format!("unsupported mix format tag {tag}")));
    }
    if channels == 0 || sample_rate == 0 || bits == 0 {
        return Err(Error::Audio("invalid mix format".into()));
    }
    Ok(SampleFormat {
        sample_rate,
        channels,
        bits_per_sample: bits,
        float,
    })
}

fn copy_packet(
    data: *mut u8,
    frames: u32,
    flags: u32,
    format: SampleFormat,
    out: &mut Vec<u8>,
) -> crate::Result<()> {
    let byte_len = usize::try_from(frames)
        .unwrap_or(0)
        .saturating_mul(format.block_align());
    if byte_len == 0 {
        return Ok(());
    }
    let silent = flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0;
    if silent || data.is_null() {
        out.resize(out.len() + byte_len, 0);
        return Ok(());
    }
    // SAFETY: GetBuffer returns `byte_len` initialized bytes until ReleaseBuffer.
    let slice = unsafe { std::slice::from_raw_parts(data, byte_len) };
    out.extend_from_slice(slice);
    Ok(())
}
