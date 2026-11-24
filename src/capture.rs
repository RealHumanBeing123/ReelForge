//! Audio capture with WASAPI through `cpal`: any input device (microphones, line-in, virtual
//! cables) and, by recording an *output* device in loopback mode, anything the computer plays
//! through any output (speakers, headphones, HDMI, Bluetooth...). Each source is written to its
//! own 32-bit float WAV file; `record::mux_args` mixes them with the screen video afterwards.
//!
//! Loopback delivers no data while nothing is playing, so every write is time-stamped and silence
//! is inserted to fill gaps. That keeps each file exactly as long as the recording.
use crate::record::AudioSource;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat};
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub struct Devices {
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

fn dedup(mut v: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    v.retain(|n| seen.insert(n.clone()));
    v
}

/// Every audio input and output device Windows reports.
pub fn list_devices() -> Devices {
    let host = cpal::default_host();
    let inputs = host.input_devices().map(|d| d.filter_map(|d| d.name().ok()).collect()).unwrap_or_default();
    let outputs = host.output_devices().map(|d| d.filter_map(|d| d.name().ok()).collect()).unwrap_or_default();
    Devices { inputs: dedup(inputs), outputs: dedup(outputs) }
}

/// The names of the default devices, to preselect.
pub fn default_names() -> (Option<String>, Option<String>) {
    let host = cpal::default_host();
    (host.default_input_device().and_then(|d| d.name().ok()), host.default_output_device().and_then(|d| d.name().ok()))
}

struct Inner {
    w: Option<hound::WavWriter<BufWriter<File>>>,
    written: u64,
}

struct Sink {
    inner: Mutex<Inner>,
    rate: u32,
    channels: u16,
    t0: Instant,
}

impl Sink {
    fn new(path: &Path, rate: u32, channels: u16) -> Result<Sink, String> {
        let spec = hound::WavSpec { channels, sample_rate: rate, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
        let w = hound::WavWriter::create(path, spec).map_err(|e| format!("Could not create {}: {e}", path.display()))?;
        Ok(Sink { inner: Mutex::new(Inner { w: Some(w), written: 0 }), rate, channels, t0: Instant::now() })
    }

    fn pad_to(&self, inner: &mut Inner, target_frames: u64) {
        if let Some(w) = inner.w.as_mut() {
            while inner.written < target_frames {
                for _ in 0..self.channels {
                    let _ = w.write_sample(0.0f32);
                }
                inner.written += 1;
            }
        }
    }

    fn push(&self, samples: impl Iterator<Item = f32>, n_samples: usize) {
        let Ok(mut g) = self.inner.lock() else { return };
        let frames = (n_samples / self.channels.max(1) as usize) as u64;
        // where this buffer started on the wall clock; fill any gap with silence first
        let start = self.t0.elapsed().as_secs_f64() - frames as f64 / self.rate as f64;
        let target = (start.max(0.0) * self.rate as f64) as u64;
        if target > g.written + (self.rate as u64 / 25) {
            self.pad_to(&mut g, target);
        }
        if let Some(w) = g.w.as_mut() {
            for s in samples {
                let _ = w.write_sample(s);
            }
            g.written += frames;
        }
    }

    fn finish(&self) {
        let Ok(mut g) = self.inner.lock() else { return };
        let total = (self.t0.elapsed().as_secs_f64() * self.rate as f64) as u64;
        self.pad_to(&mut g, total);
        if let Some(w) = g.w.take() {
            let _ = w.finalize();
        }
    }
}

fn build<T>(dev: &cpal::Device, cfg: &cpal::StreamConfig, sink: Arc<Sink>) -> Result<cpal::Stream, String>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    dev.build_input_stream(
        cfg,
        move |data: &[T], _| sink.push(data.iter().map(|s| s.to_sample::<f32>()), data.len()),
        |e| eprintln!("audio capture error: {e}"),
        None,
    )
    .map_err(|e| e.to_string())
}

fn find(devices: impl Iterator<Item = cpal::Device>, name: &str) -> Option<cpal::Device> {
    devices.into_iter().find(|d| d.name().map_or(false, |n| n == name))
}

/// Open one source and start it. `loopback` records what an output device is playing.
fn open(name: &str, loopback: bool, path: &Path) -> Result<(cpal::Stream, Arc<Sink>), String> {
    let host = cpal::default_host();
    let dev = (if loopback { find(host.output_devices().map_err(|e| e.to_string())?, name) } else { find(host.input_devices().map_err(|e| e.to_string())?, name) })
    .ok_or_else(|| format!("Audio device \"{name}\" was not found. Press Refresh devices."))?;
    let supported = (if loopback { dev.default_output_config() } else { dev.default_input_config() })
    .map_err(|e| format!("\"{name}\": {e}"))?;
    let (rate, channels, fmt) = (supported.sample_rate().0, supported.channels(), supported.sample_format());
    let cfg: cpal::StreamConfig = supported.into();
    let sink = Arc::new(Sink::new(path, rate, channels)?);
    let stream = match fmt {
        SampleFormat::F32 => build::<f32>(&dev, &cfg, sink.clone()),
        SampleFormat::I16 => build::<i16>(&dev, &cfg, sink.clone()),
        SampleFormat::U16 => build::<u16>(&dev, &cfg, sink.clone()),
        SampleFormat::I32 => build::<i32>(&dev, &cfg, sink.clone()),
        other => Err(format!("Unsupported sample format {other:?}")),
    }
    .map_err(|e| format!("Could not record \"{name}\": {e}"))?;
    stream.play().map_err(|e| format!("Could not start \"{name}\": {e}"))?;
    Ok((stream, sink))
}

pub struct AudioCapture {
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<Vec<PathBuf>>>,
}

impl AudioCapture {
    /// Start capturing. `mic` is an input device name; `system` is an output device name that is
    /// recorded in loopback. Streams live on their own thread (they cannot be moved between threads).
    pub fn start(mic: Option<&str>, system: Option<&str>, dir: &Path, stem: &str) -> Result<AudioCapture, String> {
        let mut jobs: Vec<(String, bool, PathBuf)> = vec![];
        if let Some(m) = mic {
            jobs.push((m.to_string(), false, dir.join(format!("{stem}_mic.wav"))));
        }
        if let Some(s) = system {
            jobs.push((s.to_string(), true, dir.join(format!("{stem}_system.wav"))));
        }
        let stop = Arc::new(AtomicBool::new(false));
        let (stop2, (tx, rx)) = (stop.clone(), channel::<Result<(), String>>());
        let handle = std::thread::spawn(move || {
            let mut open_streams = vec![];
            let mut paths = vec![];
            for (name, lb, path) in &jobs {
                match open(name, *lb, path) {
                    Ok(s) => {
                        open_streams.push(s);
                        paths.push(path.clone());
                    }
                    Err(e) => {
                        for (_, sink) in &open_streams {
                            sink.finish();
                        }
                        let _ = tx.send(Err(e));
                        return vec![];
                    }
                }
            }
            let _ = tx.send(Ok(()));
            while !stop2.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(40));
            }
            for (stream, sink) in open_streams {
                drop(stream);
                sink.finish();
            }
            paths
        });
        match rx.recv_timeout(Duration::from_secs(8)) {
            Ok(Ok(())) => Ok(AudioCapture { stop, handle: Some(handle) }),
            Ok(Err(e)) => {
                let _ = handle.join();
                Err(e)
            }
            Err(_) => {
                stop.store(true, Ordering::Relaxed);
                Err("Audio devices did not respond.".to_string())
            }
        }
    }
}

impl AudioSource for AudioCapture {
    fn finish(self: Box<Self>) -> Result<Vec<PathBuf>, String> {
        self.stop.store(true, Ordering::Relaxed);
        let mut me = self;
        me.handle.take().ok_or("already finished")?.join().map_err(|_| "audio thread crashed".to_string())
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
