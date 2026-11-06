//! The microphone, captured in memory while the prompt is dictated: one recording at a time, its
//! samples mixed down to mono as they come, its level kept for the window's meter, the whole
//! resampled to 16 kHz when it stops. Nothing is written to disk.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use prism_base::{tr, i18n::tr};

/// The rate both engines read: whisper's, and the one the system's recognition is given.
pub const RATE: u32 = 16_000;

/// What the capture shares with its stream's callback.
struct Shared {
    /// Mono samples at the device's rate.
    samples: Mutex<Vec<f32>>,
    /// The last callback's peak, as `f32` bits (0–1).
    level: AtomicU32,
    /// Past `max_seconds`: samples are no longer kept.
    full: AtomicBool,
    /// The stream failed (the device unplugged).
    failed: Mutex<Option<String>>,
    cap: usize,
}

/// A recording under way: its stream lives on its own thread (cpal's streams are not `Send` on
/// every system), stopped when this is stopped or dropped.
pub struct Recording {
    shared: Arc<Shared>,
    stop: Option<mpsc::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
    rate: u32,
    started: Instant,
    /// The microphone's name, as the system shows it.
    pub device: String,
}

/// The input devices' names, the default one first.
pub fn devices() -> Vec<String> {
    let host = cpal::default_host();
    let default = host.default_input_device().and_then(|d| d.description().ok()).map(|d| d.name().to_string());
    let mut names: Vec<String> = host.input_devices().map(|ds| ds.filter_map(|d| d.description().ok().map(|d| d.name().to_string())).collect()).unwrap_or_default();
    names.dedup();
    if let Some(d) = default {
        names.retain(|n| *n != d);
        names.insert(0, d);
    }
    names
}

impl Recording {
    /// Starts recording from `device` (its name; empty: the system's default), up to `max_seconds`.
    pub fn start(device: &str, max_seconds: u32) -> Result<Recording> {
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(Arc<Shared>, u32, String)>>();
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let wanted = device.trim().to_string();
        let max = max_seconds.max(1);
        let thread = std::thread::Builder::new().name("dictation-capture".into()).spawn(move || {
            let stream = match open(&wanted, max) {
                Ok((stream, shared, rate, name)) => {
                    let _ = ready_tx.send(Ok((shared, rate, name)));
                    stream
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                    return;
                }
            };
            // Until stopped, or the recording dropped (its sender with it).
            let _ = stop_rx.recv();
            drop(stream);
        })?;
        let (shared, rate, device) = ready_rx.recv_timeout(Duration::from_secs(10)).map_err(|_| anyhow!(tr("The microphone did not answer")))??;
        Ok(Recording { shared, stop: Some(stop_tx), thread: Some(thread), rate, started: Instant::now(), device })
    }

    /// Seconds recorded so far.
    pub fn seconds(&self) -> f32 {
        self.started.elapsed().as_secs_f32()
    }

    /// The level of the last moment, 0–1.
    pub fn level(&self) -> f32 {
        f32::from_bits(self.shared.level.load(Ordering::Relaxed))
    }

    /// `max_seconds` reached: what follows is not kept.
    pub fn full(&self) -> bool {
        self.shared.full.load(Ordering::Relaxed)
    }

    /// Why the stream stopped on its own, if it did.
    pub fn failed(&self) -> Option<String> {
        self.shared.failed.lock().ok().and_then(|f| f.clone())
    }

    /// Stops the recording: its samples, mono at 16 kHz.
    pub fn finish(mut self) -> Vec<f32> {
        self.halt();
        let samples = std::mem::take(&mut *self.shared.samples.lock().unwrap_or_else(|e| e.into_inner()));
        crate::resample::to_rate(&samples, self.rate, RATE)
    }

    fn halt(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Recording {
    /// A recording dropped (cancelled) is stopped, and what it heard thrown away with it.
    fn drop(&mut self) {
        self.halt();
        if let Ok(mut s) = self.shared.samples.lock() {
            s.clear();
            s.shrink_to_fit();
        }
    }
}

/// The device's stream, recording into the shared state.
fn open(wanted: &str, max_seconds: u32) -> Result<(cpal::Stream, Arc<Shared>, u32, String)> {
    let host = cpal::default_host();
    let device = if wanted.is_empty() {
        host.default_input_device()
    } else {
        host.input_devices()?.find(|d| d.description().is_ok_and(|n| n.name() == wanted)).or_else(|| host.default_input_device())
    }
    .ok_or_else(|| anyhow!(tr("No microphone found")))?;
    let name = device.description().map(|d| d.name().to_string()).unwrap_or_default();
    let config = device.default_input_config().with_context(|| tr!("{device}: no input format", device = name))?;
    let rate = config.sample_rate();
    let channels = config.channels().max(1) as usize;
    let shared = Arc::new(Shared { samples: Mutex::new(Vec::with_capacity(rate as usize * 30)), level: AtomicU32::new(0), full: AtomicBool::new(false), failed: Mutex::new(None), cap: rate as usize * max_seconds as usize });
    let stream = match config.sample_format() {
        SampleFormat::F32 => build::<f32>(&device, &config.config(), channels, &shared),
        SampleFormat::I16 => build::<i16>(&device, &config.config(), channels, &shared),
        SampleFormat::I32 => build::<i32>(&device, &config.config(), channels, &shared),
        SampleFormat::U16 => build::<u16>(&device, &config.config(), channels, &shared),
        SampleFormat::I8 => build::<i8>(&device, &config.config(), channels, &shared),
        SampleFormat::U8 => build::<u8>(&device, &config.config(), channels, &shared),
        SampleFormat::F64 => build::<f64>(&device, &config.config(), channels, &shared),
        other => return Err(anyhow!(tr!("{device}: unsupported sample format {format}", device = name, format = other.to_string()))),
    }?;
    stream.play().with_context(|| tr!("{device}: could not start recording", device = name))?;
    Ok((stream, shared, rate, name))
}

fn build<T>(device: &cpal::Device, config: &cpal::StreamConfig, channels: usize, shared: &Arc<Shared>) -> Result<cpal::Stream>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let data = shared.clone();
    let errors = shared.clone();
    let stream = device.build_input_stream(
        *config,
        move |input: &[T], _: &cpal::InputCallbackInfo| {
            let mono = crate::resample::mono(input.iter().map(|s| <f32 as FromSample<T>>::from_sample_(*s)), channels);
            let peak = mono.iter().fold(0f32, |m, s| m.max(s.abs())).min(1.0);
            data.level.store(peak.to_bits(), Ordering::Relaxed);
            if let Ok(mut kept) = data.samples.lock() {
                let room = data.cap.saturating_sub(kept.len());
                if room < mono.len() {
                    data.full.store(true, Ordering::Relaxed);
                }
                kept.extend_from_slice(&mono[..room.min(mono.len())]);
            }
        },
        move |e: cpal::Error| {
            tracing::warn!("dictation: microphone stream: {e}");
            if let Ok(mut f) = errors.failed.lock() {
                f.get_or_insert_with(|| e.to_string());
            }
        },
        None,
    )?;
    Ok(stream)
}
