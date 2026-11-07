//! Streaming preview playback.
//!
//! One FFmpeg process composites every visible layer at low resolution and pipes raw RGBA frames;
//! a second process mixes the audio and pipes 48 kHz stereo PCM into rodio. The UI picks the
//! frame matching its clock, dropping late frames, so playback keeps real-time speed.

use crate::model::Project;
use crate::render::build_graph;
use crate::util::command;
use eframe::egui;
use rodio::{OutputStreamHandle, Sink, Source};
use std::io::Read;
use std::process::{Child, Stdio};
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender};
use std::time::Duration;


pub struct Playback {
    pub frames: Receiver<Vec<u8>>,
    pub w: usize,
    pub h: usize,
    pub fps: u32,
    /// Timeline time at which this session started.
    pub t0: f64,
    video: Child,
    audio: Option<Child>,
    sink: Option<Sink>,
}

impl Drop for Playback {
    fn drop(&mut self) {
        if let Some(s) = &self.sink {
            s.stop();
        }
        let _ = self.video.kill();
        let _ = self.video.wait();
        if let Some(a) = &mut self.audio {
            let _ = a.kill();
            let _ = a.wait();
        }
    }
}

struct PcmSource {
    rx: Receiver<Vec<i16>>,
    buf: Vec<i16>,
    pos: usize,
}

impl Iterator for PcmSource {
    type Item = i16;
    fn next(&mut self) -> Option<i16> {
        while self.pos >= self.buf.len() {
            match self.rx.recv_timeout(Duration::from_millis(100)) {
                Ok(b) => {
                    self.buf = b;
                    self.pos = 0;
                }
                // Underrun: emit silence rather than stalling the audio device.
                Err(RecvTimeoutError::Timeout) => return Some(0),
                Err(RecvTimeoutError::Disconnected) => return None,
            }
        }
        let v = self.buf[self.pos];
        self.pos += 1;
        Some(v)
    }
}

impl Source for PcmSource {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        2
    }
    fn sample_rate(&self) -> u32 {
        48_000
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

fn spawn_video_reader(mut child_out: impl Read + Send + 'static, frame_bytes: usize, tx: SyncSender<Vec<u8>>, ctx: egui::Context) {
    std::thread::spawn(move || loop {
        let mut buf = vec![0u8; frame_bytes];
        if child_out.read_exact(&mut buf).is_err() {
            break;
        }
        if tx.send(buf).is_err() {
            break;
        }
        ctx.request_repaint();
    });
}

fn spawn_audio_reader(mut child_out: impl Read + Send + 'static, tx: SyncSender<Vec<i16>>) {
    std::thread::spawn(move || {
        let mut raw = vec![0u8; 16 * 1024];
        loop {
            match child_out.read(&mut raw) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let n = n & !1;
                    let samples: Vec<i16> = raw[..n].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
                    if tx.send(samples).is_err() {
                        break;
                    }
                }
            }
        }
    });
}

/// Start playback from timeline time `t0`. Returns an error string if FFmpeg can't start.
pub fn start(ctx: &egui::Context, project: &Project, t0: f64, audio_out: Option<&OutputStreamHandle>, short_side: u32) -> Result<Playback, String> {
    let win = project.window_from(t0);
    if win.duration() <= 0.0 {
        return Err("Nothing to play after the playhead.".to_string());
    }
    let fps = project.fps.clamp(1, 60);
    let (w, h) = project.output_size(short_side);
    let (w, h) = (w as usize, h as usize);

    // ---- video process ----
    let (inputs, graph, _) = build_graph(&win, w as u32, h as u32, fps, true, false);
    let mut args: Vec<String> = ["-v", "error", "-nostdin"].iter().map(|s| s.to_string()).collect();
    args.extend(inputs);
    args.extend(["-filter_complex".to_string(), graph, "-map".to_string(), "[vout]".to_string()]);
    args.extend(["-sws_flags", "bicubic+accurate_rnd+full_chroma_int", "-an", "-f", "rawvideo", "-pix_fmt", "rgba", "-r"].iter().map(|s| s.to_string()));
    args.push(fps.to_string());
    args.push("-".to_string());
    let mut video = command("ffmpeg")
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Could not start ffmpeg: {e}"))?;
    let vout = video.stdout.take().ok_or("no ffmpeg stdout")?;
    let (ftx, frx) = sync_channel::<Vec<u8>>(8);
    spawn_video_reader(vout, w * h * 4, ftx, ctx.clone());

    // ---- audio process (optional) ----
    let mut audio_child = None;
    let mut sink = None;
    if let Some(handle) = audio_out {
        let (ains, agraph, _) = build_graph(&win, w as u32, h as u32, fps, false, true);
        let mut aargs: Vec<String> = ["-v", "error", "-nostdin"].iter().map(|s| s.to_string()).collect();
        aargs.extend(ains);
        aargs.extend(["-filter_complex".to_string(), agraph, "-map".to_string(), "[aout]".to_string()]);
        aargs.extend(["-vn", "-f", "s16le", "-ar", "48000", "-ac", "2", "-"].iter().map(|s| s.to_string()));
        if let Ok(mut ac) = command("ffmpeg").args(&aargs).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn() {
            if let Some(aout) = ac.stdout.take() {
                let (atx, arx) = sync_channel::<Vec<i16>>(64);
                spawn_audio_reader(aout, atx);
                if let Ok(s) = Sink::try_new(handle) {
                    s.append(PcmSource { rx: arx, buf: vec![], pos: 0 });
                    sink = Some(s);
                }
            }
            audio_child = Some(ac);
        }
    }

    Ok(Playback { frames: frx, w, h, fps, t0, video, audio: audio_child, sink })
}
