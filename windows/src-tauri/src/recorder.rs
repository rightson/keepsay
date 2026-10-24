//! Microphone capture with cpal (WASAPI), to memory only (INV-5).
//!
//! The cpal stream lives on its own thread for the whole recording; samples
//! are downmixed to mono at the device rate into a buffer in memory. Nothing
//! is ever written to disk. The buffer is zeroed when the recording is
//! cancelled and after it has been resampled (`audio::finish`).

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample, StreamConfig};
use zeroize::Zeroize;

use crate::audio;
use crate::lock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecError {
    /// No input device.
    NoDevice,
    /// The device refused (privacy setting off, in use, unplugged).
    Unavailable,
    /// A sample format the app does not convert.
    Format,
}

impl RecError {
    pub fn code(self) -> &'static str {
        match self {
            RecError::NoDevice => "mic_none",
            RecError::Unavailable => "mic_unavailable",
            RecError::Format => "mic_format",
        }
    }
}

/// Mono PCM at the device sample rate.
pub struct Captured {
    pub pcm: Vec<f32>,
    pub rate: u32,
}

impl Drop for Captured {
    fn drop(&mut self) {
        self.pcm.zeroize();
    }
}

pub struct Recorder {
    stop_tx: mpsc::Sender<()>,
    join: JoinHandle<Result<Captured, RecError>>,
    level: Arc<AtomicU32>,
    at_cap: Arc<AtomicBool>,
}

struct Shared {
    buf: Mutex<Vec<f32>>,
    level: Arc<AtomicU32>,
    at_cap: Arc<AtomicBool>,
    channels: u16,
    cap: usize,
}

impl Recorder {
    /// Open the default input device and start capturing.
    pub fn start() -> Result<Recorder, RecError> {
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), RecError>>();
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let level = Arc::new(AtomicU32::new(0));
        let at_cap = Arc::new(AtomicBool::new(false));
        let (lvl, cap_flag) = (level.clone(), at_cap.clone());

        let join = thread::Builder::new()
            .name("keepsay-audio".into())
            .spawn(move || {
                let (stream, shared, rate) = match open(lvl, cap_flag) {
                    Ok(v) => v,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return Err(e);
                    }
                };
                let _ = ready_tx.send(Ok(()));
                // Until stop() or the Recorder is dropped.
                let _ = stop_rx.recv();
                drop(stream);
                let pcm = std::mem::take(&mut *lock(&shared.buf));
                Ok(Captured { pcm, rate })
            })
            .map_err(|_| RecError::Unavailable)?;

        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Recorder {
                stop_tx,
                join,
                level,
                at_cap,
            }),
            Ok(Err(e)) => {
                let _ = join.join();
                Err(e)
            }
            Err(_) => Err(RecError::Unavailable),
        }
    }

    /// Latest block RMS, 0.0 – 1.0.
    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }

    /// The buffer reached `audio::MAX_RECORDING_SECS`.
    pub fn at_cap(&self) -> bool {
        self.at_cap.load(Ordering::Relaxed)
    }

    pub fn stop(self) -> Result<Captured, RecError> {
        let _ = self.stop_tx.send(());
        self.join.join().map_err(|_| RecError::Unavailable)?
    }

    /// Stop and discard (Esc). The buffer is zeroed by `Captured::drop`.
    pub fn cancel(self) {
        drop(self.stop());
    }
}

fn open(
    level: Arc<AtomicU32>,
    at_cap: Arc<AtomicBool>,
) -> Result<(cpal::Stream, Arc<Shared>, u32), RecError> {
    let host = cpal::default_host();
    let device = host.default_input_device().ok_or(RecError::NoDevice)?;
    let supported = device
        .default_input_config()
        .map_err(|_| RecError::Unavailable)?;
    let format = supported.sample_format();
    let config: StreamConfig = supported.config();
    let rate = config.sample_rate;
    let shared = Arc::new(Shared {
        buf: Mutex::new(Vec::with_capacity(rate as usize * 30)),
        level,
        at_cap,
        channels: config.channels,
        cap: audio::cap_for_rate(rate),
    });
    let stream = match format {
        SampleFormat::F32 => build::<f32>(&device, config, shared.clone()),
        SampleFormat::I16 => build::<i16>(&device, config, shared.clone()),
        SampleFormat::U16 => build::<u16>(&device, config, shared.clone()),
        SampleFormat::I32 => build::<i32>(&device, config, shared.clone()),
        _ => Err(RecError::Format),
    }?;
    stream.play().map_err(|_| RecError::Unavailable)?;
    tracing::info!(rate, channels = shared.channels, "recording started");
    Ok((stream, shared, rate))
}

fn build<T>(
    device: &cpal::Device,
    config: StreamConfig,
    shared: Arc<Shared>,
) -> Result<cpal::Stream, RecError>
where
    T: SizedSample + Send + 'static,
    f32: FromSample<T>,
{
    let mut scratch: Vec<f32> = Vec::new();
    device
        .build_input_stream::<T, _, _>(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                scratch.clear();
                scratch.extend(data.iter().map(|s| s.to_sample::<f32>()));
                let mut buf = lock(&shared.buf);
                let lvl = audio::append_mono(&scratch, shared.channels, &mut buf, shared.cap);
                if buf.len() >= shared.cap {
                    shared.at_cap.store(true, Ordering::Relaxed);
                }
                drop(buf);
                scratch.zeroize();
                shared.level.store(lvl.to_bits(), Ordering::Relaxed);
            },
            |_| tracing::warn!("audio stream error"),
            None,
        )
        .map_err(|_| RecError::Unavailable)
}
