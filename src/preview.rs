use crate::model::Project;
use crate::render::build_graph;
use crate::util::command;
use eframe::egui;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::mpsc::{channel, Receiver, Sender};

pub struct PreviewReq {
    pub project: Project,
    /// Timeline time to show.
    pub t: f64,
    pub w: u32,
    pub h: u32,
}

pub struct PreviewFrame {
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
}

pub struct Thumb {
    pub media_id: u64,
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
}

/// Decode one frame through FFmpeg, scaled to fit inside w x h.
pub fn grab_frame(path: &Path, time: Option<f64>, w: u32, h: u32) -> Option<image::RgbaImage> {
    let mut cmd = command("ffmpeg");
    cmd.args(["-v", "error", "-nostdin"]);
    if let Some(t) = time {
        cmd.args(["-ss", &format!("{:.3}", t.max(0.0))]);
    }
    let out = cmd
        .arg("-i")
        .arg(path)
        .args([
            "-frames:v",
            "1",
            "-vf",
            &format!("scale={w}:{h}:force_original_aspect_ratio=decrease"),
            "-f",
            "image2pipe",
            "-vcodec",
            "png",
            "-",
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if out.stdout.is_empty() {
        return None;
    }
    image::load_from_memory(&out.stdout).ok().map(|i| i.to_rgba8())
}

/// Render one composited frame of the timeline at `t` using the same filter graph as export,
/// so effects, titles, transitions and animation are all visible in the preview.
pub fn render_still(project: &Project, t: f64, w: u32, h: u32) -> Option<image::RgbaImage> {
    let mut win = project.window_from(t);
    // Only clips on screen at the first frame matter.
    win.clips.retain(|c| c.start <= 1e-6);
    let fps = project.fps.clamp(1, 60);
    let (inputs, graph, _) = build_graph(&win, w, h, fps, true, false);
    let out = command("ffmpeg")
        .args(["-v", "error", "-nostdin"])
        .args(inputs)
        .args(["-filter_complex", &graph, "-map", "[vout]", "-sws_flags", "bicubic+accurate_rnd+full_chroma_int", "-frames:v", "1", "-f", "image2pipe", "-vcodec", "png", "-"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if out.stdout.is_empty() {
        return None;
    }
    image::load_from_memory(&out.stdout).ok().map(|i| i.to_rgba8())
}

/// Background worker that composites preview frames. Newer requests replace stale ones.
pub fn spawn_preview(ctx: egui::Context) -> (Sender<PreviewReq>, Receiver<PreviewFrame>) {
    let (req_tx, req_rx) = channel::<PreviewReq>();
    let (frame_tx, frame_rx) = channel::<PreviewFrame>();
    std::thread::spawn(move || {
        while let Ok(mut req) = req_rx.recv() {
            while let Ok(newer) = req_rx.try_recv() {
                req = newer;
            }
            let (w, h) = (req.w as usize, req.h as usize);
            let rgba = match render_still(&req.project, req.t, req.w, req.h) {
                Some(img) if img.width() as usize == w && img.height() as usize == h => img.into_raw(),
                _ => {
                    let mut black = vec![0u8; w * h * 4];
                    for px in black.chunks_exact_mut(4) {
                        px[3] = 255;
                    }
                    black
                }
            };
            if frame_tx.send(PreviewFrame { w, h, rgba }).is_err() {
                break;
            }
            ctx.request_repaint();
        }
    });
    (req_tx, frame_rx)
}

pub fn spawn_thumbnail(ctx: egui::Context, tx: Sender<Thumb>, media_id: u64, path: PathBuf, is_image: bool, duration: f64) {
    std::thread::spawn(move || {
        let t = if is_image { None } else { Some((duration * 0.1).min(1.0)) };
        if let Some(img) = grab_frame(&path, t, 192, 108) {
            let _ = tx.send(Thumb {
                media_id,
                w: img.width() as usize,
                h: img.height() as usize,
                rgba: img.into_raw(),
            });
            ctx.request_repaint();
        }
    });
}
