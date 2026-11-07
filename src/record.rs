//! Screen and voice recording.
//!
//! Capture is done by FFmpeg: `gdigrab` for the screen, DirectShow (`dshow`) for microphones
//! and loopback devices. The encoder options come from the same `encoder_args` the exporter
//! uses, so recordings offer the same codecs, bitrates and containers.
//!
//! This module has no UI dependencies.

use crate::render::{encoder_args, AudioCodec, RenderSettings};
use crate::util::command;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CaptureMode {
    Desktop,
    Region,
    Window,
}

#[derive(Clone, Debug)]
pub struct ScreenOptions {
    pub mode: CaptureMode,
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub window_title: String,
    pub draw_mouse: bool,
    pub mic: Option<String>,
    pub system_audio: Option<String>,
}

impl Default for ScreenOptions {
    fn default() -> Self {
        Self {
            mode: CaptureMode::Desktop,
            x: 0,
            y: 0,
            w: 1280,
            h: 720,
            window_title: String::new(),
            draw_mouse: true,
            mic: None,
            system_audio: None,
        }
    }
}

/// File extension for an audio-only recording with this codec.
pub fn audio_ext(c: AudioCodec) -> &'static str {
    match c {
        AudioCodec::Aac => "m4a",
        AudioCodec::Mp3 => "mp3",
        AudioCodec::Opus => "opus",
        AudioCodec::Vorbis => "ogg",
        AudioCodec::Flac => "flac",
        AudioCodec::Pcm => "wav",
    }
}

// ------------------------------------------------------------------ devices

/// Parse the output of `ffmpeg -list_devices true -f dshow -i dummy` (both the older
/// sectioned format and the newer per-line "(audio)" format).
pub fn parse_dshow_audio(text: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    let mut in_audio = false;
    for line in text.lines() {
        let l = line.trim();
        if l.contains("DirectShow audio devices") {
            in_audio = true;
            continue;
        }
        if l.contains("DirectShow video devices") {
            in_audio = false;
            continue;
        }
        if l.contains("Alternative name") {
            continue;
        }
        let Some(q1) = l.find('"') else { continue };
        let Some(len) = l[q1 + 1..].find('"') else { continue };
        let name = &l[q1 + 1..q1 + 1 + len];
        let is_audio = l.contains("(audio)") || (in_audio && !l.contains("(video)") && !l.contains("(none)"));
        if is_audio && !name.is_empty() && !out.iter().any(|n| n == name) {
            out.push(name.to_string());
        }
    }
    out
}

pub fn list_audio_devices() -> Vec<String> {
    let out = command("ffmpeg")
        .args(["-hide_banner", "-list_devices", "true", "-f", "dshow", "-i", "dummy"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .output();
    match out {
        Ok(o) => parse_dshow_audio(&String::from_utf8_lossy(&o.stderr)),
        Err(_) => vec![],
    }
}

// ------------------------------------------------------------------ command building

pub fn screen_inputs(o: &ScreenOptions, fps: u32) -> Vec<String> {
    let mut a: Vec<String> = ["-f", "gdigrab", "-framerate"].iter().map(|s| s.to_string()).collect();
    a.push(fps.to_string());
    a.extend(["-draw_mouse", if o.draw_mouse { "1" } else { "0" }, "-rtbufsize", "512M"].iter().map(|s| s.to_string()));
    if o.mode == CaptureMode::Region {
        // gdigrab needs even dimensions for yuv420p output
        let (w, h) = (o.w.max(16) & !1, o.h.max(16) & !1);
        a.extend(["-offset_x".to_string(), o.x.to_string(), "-offset_y".to_string(), o.y.to_string(), "-video_size".to_string(), format!("{w}x{h}")]);
    }
    a.push("-i".to_string());
    a.push(match o.mode {
        CaptureMode::Window => format!("title={}", o.window_title),
        _ => "desktop".to_string(),
    });
    a
}

pub fn audio_inputs(devices: &[String]) -> Vec<String> {
    let mut a = vec![];
    for d in devices {
        a.extend(["-f", "dshow", "-thread_queue_size", "512", "-i"].iter().map(|s| s.to_string()));
        a.push(format!("audio={d}"));
    }
    a
}

/// Assemble the full FFmpeg command. `inputs` holds the video input first (when `has_video`)
/// followed by `n_audio` audio inputs.
pub fn build_record_args(inputs: Vec<String>, has_video: bool, n_audio: usize, s: &RenderSettings, out: &Path) -> Vec<String> {
    let mut a: Vec<String> = ["-y", "-hide_banner", "-v", "error"].iter().map(|s| s.to_string()).collect();
    a.extend(inputs);

    let mut graph: Vec<String> = vec![];
    if has_video {
        let scale = if s.short_side > 0 {
            // Scale down to the chosen short side, never up; -2 keeps the other side even.
            let n = s.short_side;
            format!("scale=w='if(gt(iw,ih),-2,min({n},iw))':h='if(gt(iw,ih),min({n},ih),-2)':flags=lanczos")
        } else {
            "scale=trunc(iw/2)*2:trunc(ih/2)*2".to_string()
        };
        graph.push(format!("[0:v]{scale}[vout]"));
    }
    let first_audio = if has_video { 1 } else { 0 };
    match n_audio {
        0 => {}
        1 => graph.push(format!("[{first_audio}:a]aresample=48000,aformat=channel_layouts=stereo[aout]")),
        _ => {
            let labels: String = (0..n_audio).map(|i| format!("[{}:a]aresample=48000,aformat=channel_layouts=stereo[r{i}];", first_audio + i)).collect();
            let ins: String = (0..n_audio).map(|i| format!("[r{i}]")).collect();
            graph.push(format!("{}{ins}amix=inputs={n_audio}:duration=longest:normalize=0[aout]", labels));
        }
    }
    a.extend(["-filter_complex".to_string(), graph.join(";")]);
    if has_video {
        a.extend(["-map".to_string(), "[vout]".to_string()]);
    }
    if n_audio > 0 {
        a.extend(["-map".to_string(), "[aout]".to_string()]);
    }
    a.extend(encoder_args(s, has_video, n_audio > 0, false));
    a.extend(["-max_muxing_queue_size".to_string(), "1024".to_string()]);
    a.push(out.to_string_lossy().into_owned());
    a
}

pub fn screen_args(o: &ScreenOptions, s: &RenderSettings, out: &Path) -> Result<Vec<String>, String> {
    if o.mode == CaptureMode::Window && o.window_title.trim().is_empty() {
        return Err("Enter the exact title of the window to capture.".to_string());
    }
    let mut devices: Vec<String> = vec![];
    if let Some(m) = &o.mic {
        devices.push(m.clone());
    }
    if let Some(sys) = &o.system_audio {
        if Some(sys) != o.mic.as_ref() {
            devices.push(sys.clone());
        }
    }
    let mut inputs = screen_inputs(o, s.fps);
    inputs.extend(audio_inputs(&devices));
    Ok(build_record_args(inputs, true, devices.len(), s, out))
}

pub fn voice_args(mic: &str, s: &RenderSettings, out: &Path) -> Result<Vec<String>, String> {
    if mic.is_empty() {
        return Err("Choose a microphone.".to_string());
    }
    let inputs = audio_inputs(&[mic.to_string()]);
    Ok(build_record_args(inputs, false, 1, s, out))
}

/// `<Videos>\ReelForge Recordings\<kind>-<unix seconds>.<ext>`; the folder is created.
pub fn default_output(kind: &str, ext: &str) -> PathBuf {
    let base = std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join("Videos")).unwrap_or_else(std::env::temp_dir).join("ReelForge Recordings");
    let _ = std::fs::create_dir_all(&base);
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    base.join(format!("{kind}-{secs}.{ext}"))
}

// ------------------------------------------------------------------ running a recording

pub struct Recorder {
    child: Child,
    pub path: PathBuf,
    pub started: Instant,
    err: Arc<Mutex<String>>,
    stop_sent: Option<Instant>,
}

impl Recorder {
    pub fn start(args: &[String], path: PathBuf) -> Result<Recorder, String> {
        let mut child = command("ffmpeg")
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Could not start ffmpeg: {e}"))?;
        let err = Arc::new(Mutex::new(String::new()));
        if let Some(mut se) = child.stderr.take() {
            let eb = err.clone();
            std::thread::spawn(move || {
                let mut s = String::new();
                let _ = se.read_to_string(&mut s);
                *eb.lock().unwrap() = s;
            });
        }
        Ok(Recorder { child, path, started: Instant::now(), err, stop_sent: None })
    }

    /// Ask FFmpeg to finish: it flushes encoders and finalises the file.
    pub fn request_stop(&mut self) {
        if self.stop_sent.is_none() {
            if let Some(stdin) = self.child.stdin.as_mut() {
                let _ = stdin.write_all(b"q");
                let _ = stdin.flush();
            }
            self.stop_sent = Some(Instant::now());
        }
    }

    pub fn stopping(&self) -> bool {
        self.stop_sent.is_some()
    }

    pub fn elapsed(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    /// `Some(result)` once FFmpeg has exited. A file with content counts as success.
    pub fn poll(&mut self) -> Option<Result<PathBuf, String>> {
        if let Some(t) = self.stop_sent {
            if t.elapsed().as_secs() > 20 {
                let _ = self.child.kill();
            }
        }
        match self.child.try_wait() {
            Ok(Some(_)) => {
                std::thread::sleep(std::time::Duration::from_millis(150));
                let ok = std::fs::metadata(&self.path).map(|m| m.len() > 1024).unwrap_or(false);
                if ok {
                    Some(Ok(self.path.clone()))
                } else {
                    let e = self.err.lock().unwrap().clone();
                    let tail: Vec<&str> = e.lines().rev().take(10).collect();
                    let tail = tail.into_iter().rev().collect::<Vec<_>>().join("\n");
                    Some(Err(if tail.is_empty() { "Recording failed to start. Check the devices and settings.".to_string() } else { tail }))
                }
            }
            Ok(None) => None,
            Err(e) => Some(Err(e.to_string())),
        }
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            self.request_stop();
            std::thread::sleep(std::time::Duration::from_millis(500));
            let _ = self.child.kill();
        }
    }
}
