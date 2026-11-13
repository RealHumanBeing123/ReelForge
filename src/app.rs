use crate::model::*;
use crate::playback::{self, Playback};
use crate::preview::{self, PreviewFrame, PreviewReq, Thumb};
use crate::probe;
use crate::record::{self, CaptureMode, Recorder, ScreenOptions};
use crate::render::{self, AudioCodec, Container, RateControl, RenderMsg, RenderSettings, Speed, RESOLUTIONS};
use crate::util::{self, fmt_time};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, TextureHandle, TextureOptions, Vec2};
use rodio::{OutputStream, OutputStreamHandle};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Instant;

mod ext;

const TRACK_H: f32 = 54.0;
const RULER_H: f32 = 24.0;
const HEADER_W: f32 = 120.0;
const MIN_CLIP: f64 = 0.1;
/// Pull range (screen pixels) when scrubbing the playhead near a clip edge.
const PLAYHEAD_SNAP_PX: f32 = 20.0;
/// Pull range when trimming a clip edge near the playhead or another clip's edge.
const EDGE_SNAP_PX: f32 = 10.0;
const QUALITY: [(&str, u32); 4] = [("Draft 360p", 360), ("Standard 540p", 540), ("High 720p", 720), ("Full HD 1080p", 1080)];

#[derive(Clone, Copy, PartialEq, Eq)]
enum RecKind {
    Screen,
    Voice,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GridMode {
    Export,
    Screen,
    Voice,
}

const MEDIA_EXTS: &[&str] = &[
    // video
    "mp4", "m4v", "mov", "mkv", "avi", "wmv", "webm", "flv", "mpg", "mpeg", "ts", "mts", "m2ts", "3gp", "ogv", "gif",
    // audio
    "mp3", "wav", "flac", "aac", "m4a", "ogg", "oga", "opus", "wma", "aiff", "aif",
    // images
    "png", "jpg", "jpeg", "bmp", "webp", "tif", "tiff",
];

#[derive(Clone, Copy, PartialEq)]
enum DragKind {
    Move,
    TrimL,
    TrimR,
}

struct ClipDrag {
    orig: Clip,
    kind: DragKind,
    accum: f64,
}

pub struct ReelForge {
    project: Project,
    project_path: Option<PathBuf>,
    undo: Vec<Project>,
    redo: Vec<Project>,

    playhead: f64,
    playing: bool,
    last_tick: Instant,
    playback: Option<Playback>,
    clock_start: Option<Instant>,
    next_idx: u64,
    pending: Option<Vec<u8>>,
    audio: Option<(OutputStream, OutputStreamHandle)>,

    selected: Option<u64>,
    selected_media: Option<u64>,

    pps: f32,
    scroll: f64,
    lane_left: f32,
    track_rects: Vec<(usize, Rect)>,
    clip_drag: Option<ClipDrag>,
    drag_media: Option<u64>,

    thumbs: HashMap<u64, TextureHandle>,
    thumb_tx: Sender<Thumb>,
    thumb_rx: Receiver<Thumb>,
    probe_tx: Sender<Result<MediaItem, String>>,
    probe_rx: Receiver<Result<MediaItem, String>>,

    preview_tx: Sender<PreviewReq>,
    preview_rx: Receiver<PreviewFrame>,
    preview_tex: Option<TextureHandle>,
    preview_inflight: bool,
    preview_dirty: bool,
    last_req_time: f64,
    edit_stamp: Option<(u64, Instant)>,
    ext: ext::Ext,
    batch: std::collections::VecDeque<RenderSettings>,
    batch_total: usize,
    batch_dir: Option<PathBuf>,
    batch_sel: std::collections::HashSet<String>,
    playback_rate: f64,
    freeze_len: f64,
    range_from: f64,
    range_to: f64,
    range_speed: f64,
    preview_quality: u32,

    revision: u64,
    saved_revision: u64,
    allow_close: bool,
    close_prompt: bool,
    snap_marker: Option<f64>,
    last_title: String,

    rec_open: Option<RecKind>,
    rec_screen: RenderSettings,
    rec_voice: RenderSettings,
    screen_opts: ScreenOptions,
    voice_mic: Option<String>,
    audio_devices: Vec<String>,
    devices_loaded: bool,
    recorder: Option<(RecKind, Recorder)>,
    rec_countdown: Option<Instant>,
    rec_msg: String,
    rec_add_to_timeline: bool,
    rec_minimize: bool,
    pending_add: Option<PathBuf>,

    export_open: bool,
    settings: RenderSettings,
    render_rx: Option<Receiver<RenderMsg>>,
    render_cancel: Arc<AtomicBool>,
    render_progress: f32,
    render_msg: String,

    ffmpeg_ok: bool,
    status: String,
}

impl ReelForge {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut v = egui::Visuals::dark();
        v.panel_fill = Color32::from_rgb(34, 36, 40);
        v.window_fill = Color32::from_rgb(40, 42, 47);
        cc.egui_ctx.set_visuals(v);

        let (thumb_tx, thumb_rx) = channel();
        let (probe_tx, probe_rx) = channel();
        let (preview_tx, preview_rx) = preview::spawn_preview(cc.egui_ctx.clone());
        let ffmpeg_ok = util::ffmpeg_available();

        Self {
            project: Project::default(),
            project_path: None,
            undo: vec![],
            redo: vec![],
            playhead: 0.0,
            playing: false,
            last_tick: Instant::now(),
            playback: None,
            clock_start: None,
            next_idx: 0,
            pending: None,
            audio: OutputStream::try_default().ok(),
            selected: None,
            selected_media: None,
            pps: 60.0,
            scroll: 0.0,
            lane_left: 0.0,
            track_rects: vec![],
            clip_drag: None,
            drag_media: None,
            thumbs: HashMap::new(),
            thumb_tx,
            thumb_rx,
            probe_tx,
            probe_rx,
            preview_tx,
            preview_rx,
            preview_tex: None,
            preview_inflight: false,
            preview_dirty: true,
            last_req_time: -1.0,
            edit_stamp: None,
            ext: ext::Ext::load(),
            batch: Default::default(),
            batch_total: 0,
            batch_dir: None,
            batch_sel: Default::default(),
            playback_rate: 1.0,
            freeze_len: 2.0,
            range_from: 0.0,
            range_to: 1.0,
            range_speed: 2.0,
            preview_quality: 720,
            revision: 0,
            saved_revision: 0,
            allow_close: false,
            close_prompt: false,
            snap_marker: None,
            last_title: String::new(),
            rec_open: None,
            rec_screen: RenderSettings { speed: Speed::Fast, short_side: 1080, ..RenderSettings::default() },
            rec_voice: RenderSettings { acodec: AudioCodec::Aac, audio_kbps: 192, ..RenderSettings::default() },
            screen_opts: ScreenOptions::default(),
            voice_mic: None,
            audio_devices: vec![],
            devices_loaded: false,
            recorder: None,
            rec_countdown: None,
            rec_msg: String::new(),
            rec_add_to_timeline: true,
            rec_minimize: true,
            pending_add: None,
            export_open: false,
            settings: RenderSettings::default(),
            render_rx: None,
            render_cancel: Arc::new(AtomicBool::new(false)),
            render_progress: 0.0,
            render_msg: String::new(),
            ffmpeg_ok,
            status: if ffmpeg_ok {
                "Ready. Import media or drag files into the window.".into()
            } else {
                "FFmpeg not found. Put ffmpeg.exe and ffprobe.exe next to reelforge.exe or on PATH.".into()
            },
        }
    }

    // ---------------------------------------------------------------- state helpers

    fn is_dirty(&self) -> bool {
        self.revision != self.saved_revision
    }

    fn push_undo(&mut self) {
        self.revision += 1;
        self.stop_playback();
        self.undo.push(self.project.clone());
        if self.undo.len() > 100 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.preview_dirty = true;
    }

    fn do_undo(&mut self) {
        if let Some(p) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.project, p));
            self.after_history_change();
        }
    }

    fn do_redo(&mut self) {
        if let Some(p) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.project, p));
            self.after_history_change();
        }
    }

    fn after_history_change(&mut self) {
        self.revision += 1;
        if let Some(id) = self.selected {
            if self.project.clip(id).is_none() {
                self.selected = None;
            }
        }
        self.preview_dirty = true;
    }

    fn split_at_playhead(&mut self) {
        let t = self.playhead;
        let target = self
            .selected
            .and_then(|id| self.project.clip(id))
            .filter(|c| t > c.start && t < c.end())
            .map(|c| c.id)
            .or_else(|| {
                self.project
                    .clips
                    .iter()
                    .filter(|c| t > c.start && t < c.end())
                    .max_by_key(|c| c.track)
                    .map(|c| c.id)
            });
        if let Some(id) = target {
            self.push_undo();
            if let Some(new_id) = self.project.split(id, t) {
                self.selected = Some(new_id);
            }
        }
    }

    /// Insert a freeze frame of the clip under the playhead (selected clip first).
    fn freeze_at_playhead(&mut self) {
        let t = self.playhead;
        let ok = |c: &&Clip| c.title.is_none() && !c.freeze && t >= c.start && t <= c.end();
        let target = self
            .selected
            .and_then(|id| self.project.clip(id))
            .filter(ok)
            .map(|c| c.id)
            .or_else(|| self.project.clips.iter().filter(ok).filter(|c| self.project.media(c.media_id).map_or(false, |m| m.kind == MediaKind::Video)).max_by_key(|c| c.track).map(|c| c.id));
        match target {
            Some(id) => {
                self.push_undo();
                match self.project.insert_freeze(id, t, self.freeze_len.max(0.1)) {
                    Some(f) => {
                        self.selected = Some(f);
                        self.status = format!("Freeze frame added ({:.1} s). Later clips on the track moved right.", self.freeze_len);
                    }
                    None => {
                        self.undo.pop();
                        self.status = "Can't freeze this clip here.".into();
                    }
                }
            }
            None => self.status = "Park the playhead over a video clip to freeze its frame.".into(),
        }
    }

    fn delete_selected(&mut self) {
        if let Some(id) = self.selected.take() {
            self.push_undo();
            self.project.remove_clip(id);
        }
    }

    /// Media backing a clip. Title clips get a synthetic image-like item so trimming works.
    fn media_for(&self, c: &Clip) -> Option<MediaItem> {
        if let Some(t) = &c.title {
            let first = t.text.lines().next().unwrap_or("").trim();
            return Some(MediaItem {
                id: 0,
                path: PathBuf::new(),
                name: format!("Title: {}", if first.is_empty() { "(empty)" } else { first }),
                kind: MediaKind::Image,
                duration: c.duration,
                width: 0,
                height: 0,
                has_audio: false,
                video_codec: String::new(),
                audio_codec: String::new(),
                tag: String::new(),
                bin: String::new(),
                proxy: None,
            });
        }
        if c.freeze {
            // Image-like (any length) but keeps the source id so the timeline shows its thumbnail.
            let mut m = self.project.media(c.media_id).cloned()?;
            m.name = format!("Freeze: {}", m.name);
            m.kind = MediaKind::Image;
            m.duration = c.duration;
            m.has_audio = false;
            return Some(m);
        }
        self.project.media(c.media_id).cloned()
    }

    fn add_title_at_playhead(&mut self) {
        self.push_undo();
        let top = self.project.tracks.len().saturating_sub(1);
        let id = self.project.add_title(top, self.playhead, TitleSpec::default());
        self.selected = Some(id);
    }

    /// Apply an edited copy of a clip. Rapid successive edits to one clip (a slider drag)
    /// share a single undo step.
    fn commit_clip_edit(&mut self, orig: &Clip, new: Clip) {
        if &new == orig {
            return;
        }
        let coalesce = matches!(self.edit_stamp, Some((id, at)) if id == new.id && at.elapsed().as_millis() < 700);
        if coalesce {
            self.revision += 1;
            self.preview_dirty = true;
            self.stop_playback();
        } else {
            self.push_undo();
        }
        self.edit_stamp = Some((new.id, Instant::now()));
        let id = new.id;
        // Changing a clip's length from the properties panel (speed, freeze length) ripples the
        // clips after it on the same track so nothing overlaps.
        let delta = new.duration - orig.duration;
        if delta.abs() > 1e-9 && (new.start - orig.start).abs() < 1e-9 {
            self.project.shift_after(new.track, orig.end() - 1e-6, delta, Some(id));
        }
        if let Some(c) = self.project.clip_mut(id) {
            *c = new;
        }
    }

    fn add_selected_media_to_timeline(&mut self, media_id: u64) {
        self.push_undo();
        let t = self.playhead;
        if let Some(id) = self.project.add_clip(media_id, 0, t) {
            self.selected = Some(id);
        }
    }

    // ---------------------------------------------------------------- import / project IO

    fn import_dialog(&mut self, ctx: &egui::Context) {
        if let Some(files) = rfd::FileDialog::new()
            .add_filter("All media", MEDIA_EXTS)
            .add_filter("All files", &["*"])
            .pick_files()
        {
            for f in files {
                self.import_path(ctx, f);
            }
        }
    }

    fn import_path(&mut self, ctx: &egui::Context, path: PathBuf) {
        let tx = self.probe_tx.clone();
        let ctx = ctx.clone();
        self.status = format!("Importing {}...", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
        std::thread::spawn(move || {
            let r = probe::probe(&path).map_err(|e| format!("{}: {e:#}", path.display()));
            let _ = tx.send(r);
            ctx.request_repaint();
        });
    }

    fn request_thumb(&self, ctx: &egui::Context, m: &MediaItem) {
        if m.kind != MediaKind::Audio {
            preview::spawn_thumbnail(ctx.clone(), self.thumb_tx.clone(), m.id, m.path.clone(), m.kind == MediaKind::Image, m.duration);
        }
    }

    /// Returns true when the project was written to disk.
    fn save_project(&mut self, as_new: bool) -> bool {
        let mut saved = false;
        let path = if as_new || self.project_path.is_none() {
            rfd::FileDialog::new().add_filter("ReelForge project", &["rforge"]).set_file_name("project.rforge").save_file()
        } else {
            self.project_path.clone()
        };
        if let Some(p) = path {
            match serde_json::to_string_pretty(&self.project).map_err(|e| e.to_string()).and_then(|s| std::fs::write(&p, s).map_err(|e| e.to_string())) {
                Ok(()) => {
                    self.status = format!("Saved {}", p.display());
                    self.project_path = Some(p);
                    self.saved_revision = self.revision;
                    saved = true;
                }
                Err(e) => self.status = format!("Save failed: {e}"),
            }
        }
        saved
    }

    fn open_project(&mut self, ctx: &egui::Context) {
        let Some(p) = rfd::FileDialog::new().add_filter("ReelForge project", &["rforge"]).pick_file() else { return };
        match std::fs::read_to_string(&p).map_err(|e| e.to_string()).and_then(|s| serde_json::from_str::<Project>(&s).map_err(|e| e.to_string())) {
            Ok(proj) => {
                self.new_project();
                self.project = proj;
                self.project_path = Some(p);
                let items = self.project.media.clone();
                let mut missing = 0;
                for m in &items {
                    if m.path.exists() {
                        self.request_thumb(ctx, m);
                    } else {
                        missing += 1;
                    }
                }
                self.status = if missing > 0 { format!("Opened project ({missing} media file(s) missing)") } else { "Project opened".into() };
            }
            Err(e) => self.status = format!("Could not open project: {e}"),
        }
    }

    fn new_project(&mut self) {
        self.project = Project::default();
        self.project_path = None;
        self.revision = 0;
        self.saved_revision = 0;
        self.undo.clear();
        self.redo.clear();
        self.thumbs.clear();
        self.selected = None;
        self.selected_media = None;
        self.playhead = 0.0;
        self.stop_playback();
        self.preview_dirty = true;
    }

    // ---------------------------------------------------------------- per-frame plumbing

    fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(r) = self.probe_rx.try_recv() {
            match r {
                Ok(mut m) => {
                    m.id = self.project.alloc_id();
                    self.status = format!("Imported {}", m.name);
                    self.request_thumb(ctx, &m);
                    let add_now = self.pending_add.as_ref() == Some(&m.path);
                    let new_id = m.id;
                    self.project.media.push(m);
                    self.revision += 1;
                    if add_now {
                        self.pending_add = None;
                        self.push_undo();
                        let t = self.playhead;
                        if let Some(cid) = self.project.add_clip(new_id, 0, t) {
                            self.selected = Some(cid);
                        }
                    }
                }
                Err(e) => self.status = format!("Import failed: {e}"),
            }
        }
        while let Ok(t) = self.thumb_rx.try_recv() {
            let img = egui::ColorImage::from_rgba_unmultiplied([t.w, t.h], &t.rgba);
            self.thumbs.insert(t.media_id, ctx.load_texture(format!("thumb{}", t.media_id), img, TextureOptions::LINEAR));
        }
        while let Ok(fr) = self.preview_rx.try_recv() {
            self.preview_inflight = false;
            self.ext_note_frame(fr.w, fr.h, &fr.rgba);
            let img = egui::ColorImage::from_rgba_unmultiplied([fr.w, fr.h], &fr.rgba);
            match &mut self.preview_tex {
                Some(t) => t.set(img, TextureOptions::LINEAR),
                None => self.preview_tex = Some(ctx.load_texture("preview", img, TextureOptions::LINEAR)),
            }
        }
        if let Some(rx) = &self.render_rx {
            let mut finished = false;
            while let Ok(m) = rx.try_recv() {
                match m {
                    RenderMsg::Progress(p) => self.render_progress = p,
                    RenderMsg::Done => {
                        self.render_msg = "Export finished.".into();
                        self.status = "Export finished.".into();
                        finished = true;
                    }
                    RenderMsg::Cancelled => {
                        self.render_msg = "Export cancelled.".into();
                        finished = true;
                    }
                    RenderMsg::Failed(e) => {
                        self.render_msg = format!("Export failed:\n{e}");
                        finished = true;
                    }
                }
            }
            if finished {
                self.render_rx = None;
                // batch export: carry on with the next item only after a clean finish
                if self.render_msg.starts_with("Export finished") {
                    if let Some(next) = self.batch.pop_front() {
                        self.render_progress = 0.0;
                        self.begin_render(ctx, next);
                    } else if self.batch_total > 0 {
                        self.render_msg = format!("Batch finished: {} file(s) exported.", self.batch_total);
                        self.status = self.render_msg.clone();
                        self.batch_total = 0;
                    }
                } else {
                    if self.batch_total > 0 {
                        self.render_msg = format!("{}\n(Batch stopped; {} item(s) not exported.)", self.render_msg, self.batch.len());
                    }
                    self.batch.clear();
                    self.batch_total = 0;
                }
            }
        }
    }

    /// Start rendering with `settings` (honouring the in-out range option).
    fn begin_render(&mut self, ctx: &egui::Context, mut settings: RenderSettings) {
        settings.normalize();
        let to_render = match (self.project.mark_in, self.project.mark_out) {
            (Some(a), Some(b)) if settings.use_range && (b - a).abs() > 0.05 => self.project.range(a.min(b), a.max(b)),
            _ => self.project.clone(),
        };
        let (rx, c) = render::start(ctx.clone(), to_render, settings);
        self.render_rx = Some(rx);
        self.render_cancel = c;
    }

    /// Queue one export per selected preset, written into `dir`.
    fn start_batch(&mut self, ctx: &egui::Context, dir: PathBuf, presets: Vec<render::ExportPreset>) {
        let stem = self.project_path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "export".to_string());
        self.batch.clear();
        for p in presets {
            let mut st = self.settings.clone();
            p.apply(&mut st);
            let safe: String = p.name.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
            st.output = Some(dir.join(format!("{stem}_{}.{}", safe.trim_matches('_'), st.container.ext())));
            self.batch.push_back(st);
        }
        self.batch_total = self.batch.len();
        if let Some(first) = self.batch.pop_front() {
            self.render_progress = 0.0;
            self.render_msg.clear();
            self.begin_render(ctx, first);
        }
    }

    fn handle_drops(&mut self, ctx: &egui::Context) {
        let files = ctx.input(|i| i.raw.dropped_files.clone());
        for f in files {
            if let Some(p) = f.path {
                self.import_path(ctx, p);
            }
        }
    }

    fn handle_keys(&mut self, ctx: &egui::Context) {
        self.ext_keys(ctx);
    }

    fn stop_playback(&mut self) {
        if self.playing || self.playback.is_some() {
            self.preview_dirty = true;
        }
        self.playing = false;
        self.playback = None; // Drop kills the FFmpeg processes and stops audio
        self.clock_start = None;
        self.pending = None;
    }

    fn toggle_play(&mut self, ctx: &egui::Context) {
        if self.playing {
            self.stop_playback();
            return;
        }
        if self.playhead >= self.project.duration() - 1e-3 {
            self.playhead = 0.0;
        }
        let handle = self.audio.as_ref().map(|(_, h)| h);
        let rate = self.playback_rate.clamp(0.25, 8.0);
        let (proj, t0) = if (rate - 1.0).abs() > 1e-6 { (self.project.scaled(rate), self.playhead / rate) } else { (self.project.clone(), self.playhead) };
        match playback::start(ctx, &proj, t0, handle, self.preview_quality) {
            Ok(pb) => {
                self.playback = Some(pb);
                self.playing = true;
                self.clock_start = None; // starts when the first frame arrives
                self.next_idx = 0;
                self.pending = None;
            }
            Err(e) => self.status = e,
        }
    }

    fn advance_playback(&mut self, ctx: &egui::Context) {
        let Some(pb) = &self.playback else { return };
        let (t0, fps, w, h) = (pb.t0, pb.fps as f64, pb.w, pb.h);
        let end = self.project.duration();

        // The clock starts once the first frame is ready, so video and audio begin together.
        if self.clock_start.is_none() {
            match pb.frames.recv_timeout(std::time::Duration::from_millis(1)) {
                Ok(first) => {
                    self.pending = Some(first);
                    self.clock_start = Some(Instant::now());
                }
                Err(_) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(16));
                    return;
                }
            }
        }
        let elapsed = self.clock_start.map_or(0.0, |c| c.elapsed().as_secs_f64());

        // Show the newest frame whose time has come; drop frames we are late for.
        let mut show: Option<Vec<u8>> = None;
        let mut disconnected = false;
        loop {
            let frame = match self.pending.take() {
                Some(f) => f,
                None => match pb.frames.try_recv() {
                    Ok(f) => f,
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                },
            };
            if self.next_idx as f64 / fps <= elapsed {
                show = Some(frame);
                self.next_idx += 1;
            } else {
                self.pending = Some(frame);
                break;
            }
        }
        if let Some(rgba) = show {
            self.ext_note_frame(w, h, &rgba);
            let img = egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba);
            match &mut self.preview_tex {
                Some(t) => t.set(img, TextureOptions::LINEAR),
                None => self.preview_tex = Some(ctx.load_texture("preview", img, TextureOptions::LINEAR)),
            }
        }

        self.playhead = ((t0 + elapsed) * self.playback_rate).min(end);
        if self.playhead >= end - 1e-3 || (disconnected && self.pending.is_none() && elapsed > 0.5 && self.next_idx as f64 / fps < elapsed) {
            self.playhead = end;
            self.stop_playback();
            self.preview_dirty = false;
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(8));
        }
    }

    fn pump_preview(&mut self) {
        if self.playing {
            return;
        }
        let moved = (self.playhead - self.last_req_time).abs() > 1e-4;
        if (self.preview_dirty || moved) && !self.preview_inflight {
            let (w, h) = self.project.output_size(self.preview_quality);
            let req = PreviewReq { project: self.project.clone(), t: self.playhead, w, h };
            if self.preview_tx.send(req).is_ok() {
                self.preview_inflight = true;
                self.preview_dirty = false;
                self.last_req_time = self.playhead;
            }
        }
    }

    // ---------------------------------------------------------------- UI: top menu

    fn menu_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New project").clicked() {
                        self.new_project();
                        ui.close_menu();
                    }
                    if ui.button("Open project...   Ctrl+O").clicked() {
                        self.open_project(ctx);
                        ui.close_menu();
                    }
                    if ui.button("Save   Ctrl+S").clicked() {
                        self.save_project(false);
                        ui.close_menu();
                    }
                    if ui.button("Save as...").clicked() {
                        self.save_project(true);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Import media...   Ctrl+I").clicked() {
                        self.import_dialog(ctx);
                        ui.close_menu();
                    }
                    if ui.button("Export...   Ctrl+E").clicked() {
                        self.export_open = true;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Exit").clicked() {
                        self.request_exit(ctx);
                        ui.close_menu();
                    }
                });
                ui.menu_button("Edit", |ui| {
                    if ui.add_enabled(!self.undo.is_empty(), egui::Button::new("Undo   Ctrl+Z")).clicked() {
                        self.do_undo();
                        ui.close_menu();
                    }
                    if ui.add_enabled(!self.redo.is_empty(), egui::Button::new("Redo   Ctrl+Y")).clicked() {
                        self.do_redo();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Split at playhead   S").clicked() {
                        self.split_at_playhead();
                        ui.close_menu();
                    }
                    if ui.button("Freeze frame at playhead   F").clicked() {
                        self.freeze_at_playhead();
                        ui.close_menu();
                    }
                    if ui.button("Delete clip   Del").clicked() {
                        self.delete_selected();
                        ui.close_menu();
                    }
                });
                ui.menu_button("Project", |ui| {
                    if ui.button("Add title at playhead").clicked() {
                        self.add_title_at_playhead();
                        ui.close_menu();
                    }
                    if ui.button("Add track").clicked() {
                        self.push_undo();
                        self.project.add_track();
                        ui.close_menu();
                    }
                });
                ui.menu_button("Record", |ui| {
                    if ui.button("Record screen...").clicked() {
                        self.rec_open = Some(RecKind::Screen);
                        ui.close_menu();
                    }
                    if ui.button("Record voice...").clicked() {
                        self.rec_open = Some(RecKind::Voice);
                        ui.close_menu();
                    }
                });
                self.ext_menus(ui, ctx);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let btn = egui::Button::new(egui::RichText::new("  Export  ").strong()).fill(Color32::from_rgb(40, 120, 70));
                    if ui.add(btn).clicked() {
                        self.export_open = true;
                    }
                });
            });
        });
    }

    // ---------------------------------------------------------------- UI: media bin

    fn media_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("media").resizable(true).default_width(250.0).min_width(180.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Media");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("+ Import").clicked() {
                        self.import_dialog(ctx);
                    }
                });
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Search");
                ui.add(egui::TextEdit::singleline(&mut self.ext.search).hint_text("name, tag or bin").desired_width(f32::INFINITY));
            });
            if self.project.media.is_empty() {
                ui.add_space(20.0);
                ui.label(egui::RichText::new("Drop video, audio or image files here,\nor click Import.").weak());
            }
            let q = self.ext.search.trim().to_lowercase();
            let items: Vec<MediaItem> = self
                .project
                .media
                .iter()
                .filter(|m| q.is_empty() || m.name.to_lowercase().contains(&q) || m.tag.to_lowercase().contains(&q) || m.bin.to_lowercase().contains(&q))
                .cloned()
                .collect();
            let mut remove: Option<u64> = None;
            let mut add: Option<u64> = None;
            // tags / bin for the selected media item
            if let Some(id) = self.selected_media {
                if let Some(idx) = self.project.media.iter().position(|m| m.id == id) {
                    let (mut tag, mut bin) = (self.project.media[idx].tag.clone(), self.project.media[idx].bin.clone());
                    ui.horizontal(|ui| {
                        ui.label("Tags");
                        ui.add(egui::TextEdit::singleline(&mut tag).desired_width(80.0));
                        ui.label("Bin");
                        ui.add(egui::TextEdit::singleline(&mut bin).desired_width(70.0));
                    });
                    if tag != self.project.media[idx].tag || bin != self.project.media[idx].bin {
                        self.project.media[idx].tag = tag;
                        self.project.media[idx].bin = bin;
                        self.revision += 1;
                    }
                }
            }
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for m in &items {
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 62.0), Sense::click_and_drag());
                    let selected = self.selected_media == Some(m.id);
                    let bg = if selected {
                        Color32::from_rgb(55, 75, 105)
                    } else if resp.hovered() {
                        Color32::from_rgb(50, 53, 58)
                    } else {
                        Color32::from_rgb(42, 44, 49)
                    };
                    let p = ui.painter();
                    p.rect_filled(rect, 4.0, bg);
                    let thumb = Rect::from_min_size(rect.min + Vec2::new(4.0, 5.0), Vec2::new(88.0, 52.0));
                    p.rect_filled(thumb, 3.0, Color32::BLACK);
                    if let Some(tex) = self.thumbs.get(&m.id) {
                        let sz = tex.size_vec2();
                        let scale = (thumb.width() / sz.x).min(thumb.height() / sz.y);
                        let r = Rect::from_center_size(thumb.center(), sz * scale);
                        p.image(tex.id(), r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                    } else {
                        let label = if m.kind == MediaKind::Audio { "AUDIO" } else { "..." };
                        p.text(thumb.center(), Align2::CENTER_CENTER, label, FontId::proportional(11.0), Color32::GRAY);
                    }
                    let tx = thumb.right() + 8.0;
                    p.text(Pos2::new(tx, rect.top() + 14.0), Align2::LEFT_CENTER, elide(&m.name, 20), FontId::proportional(13.0), Color32::WHITE);
                    let info = match m.kind {
                        MediaKind::Image => format!("Image  {}x{}", m.width, m.height),
                        MediaKind::Audio => format!("Audio  {}", fmt_time(m.duration)),
                        MediaKind::Video => format!("{}x{}  {}", m.width, m.height, fmt_time(m.duration)),
                    };
                    p.text(Pos2::new(tx, rect.top() + 32.0), Align2::LEFT_CENTER, info, FontId::proportional(11.0), Color32::GRAY);
                    let codecs = format!("{} {}", m.video_codec, m.audio_codec);
                    let extra = [codecs.trim().to_string(), if m.bin.is_empty() { String::new() } else { format!("[{}]", m.bin) }, if m.proxy.is_some() { "proxy".to_string() } else { String::new() }].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join("  ");
                    p.text(Pos2::new(tx, rect.top() + 48.0), Align2::LEFT_CENTER, extra, FontId::proportional(10.0), Color32::from_gray(120));

                    if resp.clicked() {
                        self.selected_media = Some(m.id);
                    }
                    if resp.double_clicked() {
                        add = Some(m.id);
                    }
                    if resp.drag_started() {
                        self.drag_media = Some(m.id);
                        self.selected_media = Some(m.id);
                    }
                    resp.context_menu(|ui| {
                        if ui.button("Add to timeline").clicked() {
                            add = Some(m.id);
                            ui.close_menu();
                        }
                        if ui.button("Remove from project").clicked() {
                            remove = Some(m.id);
                            ui.close_menu();
                        }
                    });
                    ui.add_space(2.0);
                }
            });
            if let Some(id) = add {
                self.add_selected_media_to_timeline(id);
            }
            if let Some(id) = remove {
                self.push_undo();
                self.project.remove_media(id);
                self.thumbs.remove(&id);
            }
        });
    }

    // ---------------------------------------------------------------- UI: properties

    fn props_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("props").resizable(true).default_width(240.0).min_width(180.0).show(ctx, |ui| {
            ui.heading("Properties");
            ui.separator();
            if let Some(id) = self.selected {
                if let Some(orig) = self.project.clip(id).cloned() {
                    let mut c = orig.clone();
                    let media = self.media_for(&c);
                    let playhead_rel = self.playhead - c.start;
                    let (mut do_split, mut do_delete, mut do_freeze) = (false, false, false);
                    let mut do_range: Option<(f64, f64, f64)> = None;
                    let mut pending: Option<ext::ExtAction> = None;
                    let fr = 1.0 / self.project.fps.max(1) as f64;
                    let next_id = self.project.clips.iter().find(|n| n.track == orig.track && n.id != orig.id && (n.start - orig.end()).abs() < 1e-3).map(|n| n.id);
                    let (ramp_s0, ramp_s1) = (&mut self.ext.ramp_s0, &mut self.ext.ramp_s1);
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        ui.label(egui::RichText::new(media.as_ref().map(|m| m.name.clone()).unwrap_or_default()).strong());
                        ui.add_space(4.0);
                        egui::Grid::new("clipinfo").num_columns(2).show(ui, |ui| {
                            ui.label("Start");
                            ui.label(fmt_time(c.start));
                            ui.end_row();
                            ui.label("Duration");
                            let free_len = c.freeze || c.title.is_some() || media.as_ref().map_or(false, |m| m.kind == MediaKind::Image);
                            if free_len {
                                ui.add(egui::DragValue::new(&mut c.duration).speed(0.05).range(MIN_CLIP..=3600.0).suffix(" s"));
                            } else {
                                ui.label(fmt_time(c.duration));
                            }
                            ui.end_row();
                        });
                        if media.as_ref().map_or(false, |m| m.has_audio && m.kind != MediaKind::Image) {
                            ui.add_space(6.0);
                            let mut vol = c.volume * 100.0;
                            if ui.add(egui::Slider::new(&mut vol, 0.0..=200.0).suffix("%").text("Volume")).changed() {
                                c.volume = vol / 100.0;
                            }
                            ui.label(egui::RichText::new(format!("Clip gain {:+.1} dB", 20.0 * (c.volume.max(0.0001) as f64).log10())).weak().small());
                            let hi = (c.duration / 2.0).max(0.5).max(c.audio_fade_in).max(c.audio_fade_out);
                            ui.add(egui::Slider::new(&mut c.audio_fade_in, 0.0..=hi).suffix(" s").text("Audio fade in"));
                            ui.add(egui::Slider::new(&mut c.audio_fade_out, 0.0..=hi).suffix(" s").text("Audio fade out"));
                            if ui.add_enabled(next_id.is_some(), egui::Button::new("Crossfade into next clip")).on_hover_text("Fades this clip's audio out and the next clip's in across the cut (1 s)").clicked() {
                                pending = Some(ext::ExtAction::Crossfade(c.id));
                            }
                            egui::CollapsingHeader::new("Audio effects").id_salt("afx").default_open(!c.afx.is_default()).show(ui, |ui| audio_fx_editor(ui, &mut c.afx));
                        }
                        if let Some(t) = c.title.as_mut() {
                            ui.add_space(8.0);
                            egui::CollapsingHeader::new("Title").default_open(true).show(ui, |ui| title_editor(ui, t));
                        }
                        let is_audio = media.as_ref().map_or(false, |m| m.kind == MediaKind::Audio);
                        let is_video = media.as_ref().map_or(false, |m| m.kind == MediaKind::Video) && c.title.is_none() && !c.freeze;
                        if is_video || (is_audio && c.title.is_none()) {
                            egui::CollapsingHeader::new("Speed & direction").id_salt("speed").default_open(true).show(ui, |ui| {
                                let mut sp = c.speed;
                                if ui.add(egui::Slider::new(&mut sp, 0.1..=16.0).logarithmic(true).suffix("x").text("Speed")).changed() {
                                    // keep the same stretch of source: length scales inversely with speed
                                    c.duration = (orig.duration * orig.speed / sp).max(MIN_CLIP);
                                    c.speed = sp;
                                }
                                ui.horizontal(|ui| {
                                    for v in [0.25, 0.5, 1.0, 2.0, 4.0] {
                                        if ui.small_button(format!("{v}x")).clicked() {
                                            c.duration = (orig.duration * orig.speed / v).max(MIN_CLIP);
                                            c.speed = v;
                                        }
                                    }
                                });
                                ui.checkbox(&mut c.reverse, "Reverse (play backwards)");
                                ui.label(egui::RichText::new("Speed changes the whole clip. Later clips on this track move to make room. Audio is sped up / slowed with it.").weak().small());
                                ui.add_space(4.0);
                                ui.label(egui::RichText::new("Change speed between two points").strong());
                                let (lo, hi) = (c.start, c.end());
                                let (mut a, mut b) = (self.range_from.clamp(lo, hi), self.range_to.clamp(lo, hi));
                                ui.horizontal(|ui| {
                                    ui.label("From");
                                    ui.add(egui::DragValue::new(&mut a).speed(0.05).range(lo..=hi).suffix(" s"));
                                    if ui.small_button("Playhead").clicked() {
                                        a = self.playhead.clamp(lo, hi);
                                    }
                                });
                                ui.horizontal(|ui| {
                                    ui.label("To    ");
                                    ui.add(egui::DragValue::new(&mut b).speed(0.05).range(lo..=hi).suffix(" s"));
                                    if ui.small_button("Playhead").clicked() {
                                        b = self.playhead.clamp(lo, hi);
                                    }
                                });
                                self.range_from = a;
                                self.range_to = b;
                                ui.add(egui::Slider::new(&mut self.range_speed, 0.1..=16.0).logarithmic(true).suffix("x").text("Speed"));
                                if ui.button("Apply to range").clicked() {
                                    do_range = Some((a, b, self.range_speed));
                                }
                                ui.label(egui::RichText::new("Splits the clip at both points and changes only the part between them.").weak().small());
                                ui.add_space(4.0);
                                ui.label(egui::RichText::new("Speed ramp").strong());
                                ui.add(egui::Slider::new(&mut *ramp_s0, 0.1..=16.0).logarithmic(true).suffix("x").text("Start speed"));
                                ui.add(egui::Slider::new(&mut *ramp_s1, 0.1..=16.0).logarithmic(true).suffix("x").text("End speed"));
                                if ui.button("Apply ramp to range").clicked() {
                                    pending = Some(ext::ExtAction::Ramp { id: c.id, a: self.range_from, b: self.range_to, s0: *ramp_s0, s1: *ramp_s1 });
                                }
                                ui.label(egui::RichText::new("Accelerates smoothly from the start to the end speed across the From-To range (10 steps).").weak().small());
                                if is_video {
                                    ui.checkbox(&mut c.interpolate, "Smooth slow motion (frame interpolation)");
                                }
                            });
                        }
                        if is_video {
                            egui::CollapsingHeader::new("Freeze frame").id_salt("freeze").default_open(true).show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label("Hold for");
                                    ui.add(egui::DragValue::new(&mut self.freeze_len).speed(0.05).range(0.1..=60.0).suffix(" s"));
                                });
                                if ui.button("Freeze frame at playhead").clicked() {
                                    do_freeze = true;
                                }
                                ui.label(egui::RichText::new("Splits the clip at the playhead and holds that picture. Later clips move right. Audio is silent during the hold.").weak().small());
                            });
                        }
                        if c.freeze {
                            ui.label(egui::RichText::new("Freeze frame: drag the clip's edges or set Duration to change how long it holds.").weak().small());
                        }
                        if c.title.is_none() && !c.freeze {
                            egui::CollapsingHeader::new("Edit tools").id_salt("edittools").default_open(false).show(ui, |ui| {
                                let mut nudge = |ui: &mut egui::Ui, label: &str, hint: &str, mk: &dyn Fn(f64) -> Option<ext::ExtAction>, pending: &mut Option<ext::ExtAction>| {
                                    ui.horizontal(|ui| {
                                        ui.label(label).on_hover_text(hint);
                                        for (txt, d) in [("-1 s", -1.0), ("-1 fr", -fr), ("+1 fr", fr), ("+1 s", 1.0)] {
                                            if ui.small_button(txt).clicked() {
                                                *pending = mk(d);
                                            }
                                        }
                                    });
                                };
                                let id = c.id;
                                nudge(ui, "Slip", "Changes which part of the source plays; the clip keeps its place and length.", &|d| Some(ext::ExtAction::Slip(id, d)), &mut pending);
                                nudge(ui, "Slide", "Moves the clip; the neighbours on either side absorb the change.", &|d| Some(ext::ExtAction::Slide(id, d)), &mut pending);
                                if let Some(r) = next_id {
                                    nudge(ui, "Roll", "Moves the cut between this clip and the next one.", &|d| Some(ext::ExtAction::Roll(id, r, d)), &mut pending);
                                } else {
                                    ui.label(egui::RichText::new("Roll needs a clip touching this one's end.").weak().small());
                                }
                            });
                        }
                        if !is_audio && c.title.is_none() {
                            egui::ComboBox::from_label("Blend mode").selected_text(c.blend.label()).show_ui(ui, |ui| {
                                for b in BlendMode::ALL {
                                    ui.selectable_value(&mut c.blend, b, b.label());
                                }
                            });
                        }
                        if !is_audio {
                            let max_d = (c.duration / 2.0).clamp(0.2, 3.0);
                            egui::CollapsingHeader::new("Transitions").default_open(true).show(ui, |ui| {
                                transition_editor(ui, "In", &mut c.transition_in, max_d, "tr_in");
                                transition_editor(ui, "Out", &mut c.transition_out, max_d, "tr_out");
                            });
                            egui::CollapsingHeader::new(format!("Effects ({})", c.effects.len())).id_salt("fx").default_open(true).show(ui, |ui| {
                                effects_editor(ui, &mut c.effects);
                            });
                            egui::CollapsingHeader::new("Animation (keyframes)").default_open(true).show(ui, |ui| {
                                anim_editor(ui, &mut c.anim, playhead_rel, c.duration);
                            });
                        }
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            do_split = ui.button("Split at playhead").clicked();
                            do_delete = ui.button("Delete clip").clicked();
                        });
                    });
                    self.commit_clip_edit(&orig, c);
                    if pending.is_some() {
                        self.ext.action = pending;
                    }
                    if do_split {
                        self.split_at_playhead();
                    }
                    if do_freeze {
                        self.freeze_at_playhead();
                    }
                    if let Some((a, b, sp)) = do_range {
                        self.push_undo();
                        match self.project.apply_speed_range(id, a, b, sp) {
                            Some(mid) => {
                                self.selected = Some(mid);
                                self.status = format!("Speed {sp:.2}x applied to the selected range.");
                            }
                            None => {
                                self.undo.pop();
                                self.status = "Pick a range at least 0.05 s long inside the clip.".into();
                            }
                        }
                    }
                    if do_delete {
                        self.delete_selected();
                    }
                    return;
                }
            }
            ui.label(egui::RichText::new("Canvas").strong());
            ui.add_space(4.0);
            let mut changed = false;
            let (mut w, mut h) = (self.project.width, self.project.height);
            let presets = [("1920 x 1080 (16:9)", 1920, 1080), ("1280 x 720 (16:9)", 1280, 720), ("3840 x 2160 (16:9)", 3840, 2160), ("1080 x 1920 (9:16)", 1080, 1920), ("1080 x 1080 (1:1)", 1080, 1080), ("1440 x 1080 (4:3)", 1440, 1080)];
            egui::ComboBox::from_id_salt("canvas").selected_text(format!("{w} x {h}")).show_ui(ui, |ui| {
                for (label, pw, ph) in presets {
                    if ui.selectable_label(w == pw && h == ph, label).clicked() {
                        w = pw;
                        h = ph;
                        changed = true;
                    }
                }
            });
            ui.add_space(6.0);
            ui.label("Frame rate");
            let mut fps = self.project.fps;
            egui::ComboBox::from_id_salt("fps").selected_text(format!("{fps} fps")).show_ui(ui, |ui| {
                for f in [24, 25, 30, 50, 60] {
                    ui.selectable_value(&mut fps, f, format!("{f} fps"));
                }
            });
            if changed || fps != self.project.fps {
                self.push_undo();
                self.project.width = w;
                self.project.height = h;
                self.project.fps = fps;
                self.settings.fps = fps;
            }
            ui.add_space(14.0);
            ui.label(egui::RichText::new("Select a clip on the timeline to edit it.").weak());
        });
    }

    // ---------------------------------------------------------------- UI: preview

    fn preview_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let transport_h = 42.0;
            let avail = ui.available_size();
            let area = Vec2::new(avail.x, (avail.y - transport_h).max(50.0));
            let (rect, _) = ui.allocate_exact_size(area, Sense::hover());
            ui.painter().rect_filled(rect, 0.0, Color32::from_rgb(20, 21, 24));
            let aspect = self.project.width as f32 / self.project.height as f32;
            let inner = rect.shrink(8.0);
            let mut size = Vec2::new(inner.width(), inner.width() / aspect);
            if size.y > inner.height() {
                size = Vec2::new(inner.height() * aspect, inner.height());
            }
            let canvas = Rect::from_center_size(inner.center(), size);
            ui.painter().rect_filled(canvas, 0.0, Color32::BLACK);
            if let Some(tex) = &self.preview_tex {
                ui.painter().image(tex.id(), canvas, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            }

            // Drag the selected title directly on the canvas. The box is an estimate of the text
            // extent (FFmpeg lays the text out), good enough to grab and move it.
            let hit = self
                .selected
                .and_then(|id| self.project.clip(id))
                .filter(|c| c.title.is_some() && self.playhead >= c.start && self.playhead < c.end())
                .cloned();
            if let (Some(orig), false) = (hit, self.playing) {
                let spec = orig.title.clone().unwrap_or_default();
                let (pw, ph) = (self.project.width as f32, self.project.height as f32);
                let size_px = (spec.size / 100.0 * ph).max(8.0);
                let pad = size_px * 0.3;
                let (tw, th) = render::title_text_extent(&spec.text, size_px);
                let est_w = ((tw + 2.0 * pad) / pw).min(1.0);
                let est_h = ((th + 2.0 * pad) / ph).min(1.0);
                let custom_box = spec.box_w > 0.0 || spec.box_h > 0.0;
                let box_w = if spec.box_w > 0.0 { spec.box_w } else { est_w };
                let box_h = if spec.box_h > 0.0 { spec.box_h } else { est_h };
                let (cx, cy) = match spec.pos {
                    Some([x, y]) => (x, y),
                    None => match spec.position {
                        TitlePos::Center => (0.5, 0.5),
                        TitlePos::LowerThird => (0.5, 0.76),
                        TitlePos::Top => (0.5, 0.08 + box_h / 2.0 - if custom_box { 0.0 } else { 0.01 }),
                    },
                };
                let centre = Pos2::new(canvas.left() + cx * canvas.width(), canvas.top() + cy * canvas.height());
                let sz = Vec2::new((box_w * canvas.width()).max(28.0), (box_h * canvas.height()).max(20.0));
                let r = Rect::from_center_size(centre, sz);
                let resp = ui.interact(r, egui::Id::new("title_drag"), Sense::drag());
                if resp.hovered() || resp.dragged() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Move);
                }
                let col = if resp.dragged() { Color32::from_rgb(255, 215, 90) } else { Color32::from_white_alpha(170) };
                ui.painter().rect_stroke(r, 2.0, Stroke::new(1.5, col));
                // corner handle: drag to resize the box about its centre
                let handle = Rect::from_center_size(r.right_bottom(), Vec2::splat(12.0));
                let hresp = ui.interact(handle, egui::Id::new("title_resize"), Sense::drag());
                ui.painter().rect_filled(handle, 2.0, col);
                if hresp.hovered() || hresp.dragged() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeNwSe);
                }
                if hresp.dragged() {
                    let d = hresp.drag_delta();
                    let mut new = orig.clone();
                    if let Some(t) = new.title.as_mut() {
                        t.box_w = (box_w + 2.0 * d.x / canvas.width()).clamp(0.05, 1.0);
                        t.box_h = (box_h + 2.0 * d.y / canvas.height()).clamp(0.05, 1.0);
                    }
                    self.commit_clip_edit(&orig, new);
                } else if resp.dragged() {
                    let d = resp.drag_delta();
                    let nx = (cx + d.x / canvas.width()).clamp(0.0, 1.0);
                    let ny = (cy + d.y / canvas.height()).clamp(0.0, 1.0);
                    let mut new = orig.clone();
                    if let Some(t) = new.title.as_mut() {
                        t.pos = Some([nx, ny]);
                    }
                    self.commit_clip_edit(&orig, new);
                }
            }

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("|<").clicked() {
                    self.stop_playback();
                    self.playhead = 0.0;
                }
                let label = if self.playing { "  Pause  " } else { "  Play  " };
                if ui.button(label).clicked() {
                    self.playback_rate = 1.0;
                    self.toggle_play(ctx);
                }
                if self.playing && (self.playback_rate - 1.0).abs() > 1e-6 {
                    ui.label(egui::RichText::new(format!("{}x", self.playback_rate)).strong());
                }
                if ui.button(">|").clicked() {
                    self.stop_playback();
                    self.playhead = self.project.duration();
                }
                ui.monospace(format!("{} / {}", fmt_time(self.playhead), fmt_time(self.project.duration())));
                let q_label = QUALITY.iter().find(|q| q.1 == self.preview_quality).map_or("Custom", |q| q.0);
                egui::ComboBox::from_id_salt("preview_quality").width(120.0).selected_text(q_label).show_ui(ui, |ui| {
                    for (label, v) in QUALITY {
                        if ui.selectable_value(&mut self.preview_quality, v, label).clicked() {
                            self.stop_playback();
                            self.preview_dirty = true;
                        }
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(&self.status).weak().size(11.0));
                });
            });
        });
    }

    // ---------------------------------------------------------------- UI: timeline

    /// Snap a trimmed edge to the playhead or another clip's edge within EDGE_SNAP_PX.
    fn snap_edge(&self, t: f64, ignore: u64) -> f64 {
        let mut pts = self.project.edge_points(Some(ignore));
        pts.push(self.playhead);
        nearest_within(pts, t, (EDGE_SNAP_PX / self.pps) as f64).unwrap_or(t)
    }

    fn snap(&self, t: f64, dur: f64, ignore: u64) -> f64 {
        let thr = 8.0 / self.pps as f64;
        let mut cands: Vec<f64> = vec![0.0, self.playhead];
        for c in self.project.clips.iter().filter(|c| c.id != ignore) {
            cands.push(c.start);
            cands.push(c.end());
        }
        let mut best: Option<(f64, f64)> = None; // (distance, delta)
        for &x in &cands {
            for (edge, off) in [(t, 0.0), (t + dur, dur)] {
                let d = (edge - x).abs();
                if d < thr && best.map_or(true, |(bd, _)| d < bd) {
                    best = Some((d, x - off - t));
                }
            }
        }
        t + best.map_or(0.0, |(_, delta)| delta)
    }

    fn timeline_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("timeline").resizable(true).default_height(290.0).min_height(160.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.add_enabled(!self.undo.is_empty(), egui::Button::new("Undo")).clicked() {
                    self.do_undo();
                }
                if ui.add_enabled(!self.redo.is_empty(), egui::Button::new("Redo")).clicked() {
                    self.do_redo();
                }
                ui.separator();
                if ui.button("Split (S)").clicked() {
                    self.split_at_playhead();
                }
                if ui.add_enabled(self.selected.is_some(), egui::Button::new("Delete")).clicked() {
                    self.delete_selected();
                }
                ui.separator();
                if ui.button("+ Title").clicked() {
                    self.add_title_at_playhead();
                }
                if ui.button("+ Track").clicked() {
                    self.push_undo();
                    self.project.add_track();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add(egui::Slider::new(&mut self.pps, 4.0..=600.0).logarithmic(true).show_value(false));
                    ui.label("Zoom");
                });
            });
            ui.separator();
            self.h_scrollbar(ui);

            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| self.timeline_body(ui));
        });
    }

    /// Horizontal scrollbar for the timeline, aligned with the track lanes. Drag the thumb, or
    /// click the bar to jump there. The scrollable length is the project plus some room to spare.
    fn h_scrollbar(&mut self, ui: &mut egui::Ui) {
        let width = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 14.0), Sense::hover());
        let bar = Rect::from_min_max(Pos2::new(rect.left() + HEADER_W + 2.0, rect.top() + 2.0), Pos2::new(rect.right() - 2.0, rect.bottom() - 2.0));
        if bar.width() < 20.0 {
            return;
        }
        let pps = self.pps as f64;
        let view = bar.width() as f64 / pps;
        let total = (self.project.duration() * 1.1 + view * 0.5).max(self.playhead + 5.0).max(self.scroll + view).max(view);
        let max_scroll = (total - view).max(0.0);
        self.scroll = self.scroll.clamp(0.0, max_scroll);
        let track_w = bar.width() as f64;
        let thumb_w = ((view / total) * track_w).clamp(24.0, track_w);
        let travel = (track_w - thumb_w).max(1.0);
        let thumb_x = bar.left() as f64 + if max_scroll > 0.0 { self.scroll / max_scroll * travel } else { 0.0 };
        let thumb = Rect::from_min_size(Pos2::new(thumb_x as f32, bar.top()), Vec2::new(thumb_w as f32, bar.height()));

        let track_resp = ui.interact(bar, egui::Id::new("hscroll_track"), Sense::click());
        let thumb_resp = ui.interact(thumb, egui::Id::new("hscroll_thumb"), Sense::drag());
        if thumb_resp.dragged() {
            self.scroll = (self.scroll + thumb_resp.drag_delta().x as f64 / travel * max_scroll).clamp(0.0, max_scroll);
        } else if track_resp.clicked() {
            if let Some(p) = track_resp.interact_pointer_pos() {
                let centre = (p.x as f64 - bar.left() as f64 - thumb_w / 2.0) / travel;
                self.scroll = (centre * max_scroll).clamp(0.0, max_scroll);
            }
        }
        if thumb_resp.hovered() || thumb_resp.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
        let p = ui.painter();
        p.rect_filled(bar, 4.0, Color32::from_gray(22));
        let col = if thumb_resp.dragged() { Color32::from_gray(150) } else if thumb_resp.hovered() { Color32::from_gray(120) } else { Color32::from_gray(90) };
        p.rect_filled(thumb.shrink2(Vec2::new(0.0, 1.0)), 4.0, col);
        // playhead position marker
        let px = bar.left() as f64 + (self.playhead / total) * track_w;
        p.line_segment([Pos2::new(px as f32, bar.top()), Pos2::new(px as f32, bar.bottom())], Stroke::new(1.5, Color32::from_rgb(255, 90, 80)));
    }

    fn timeline_body(&mut self, ui: &mut egui::Ui) {
        let n = self.project.tracks.len();
        let width = ui.available_width();
        let height = RULER_H + n as f32 * TRACK_H;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
        let lane_left = rect.left() + HEADER_W;
        self.lane_left = lane_left;
        let lane_area = Rect::from_min_max(Pos2::new(lane_left, rect.top()), rect.max);
        let painter = ui.painter_at(rect);
        let lane_painter = ui.painter_at(lane_area);
        let pps = self.pps as f64;
        let tx = |t: f64, scroll: f64| lane_left + ((t - scroll) * pps) as f32;

        // zoom / horizontal scroll
        if ui.rect_contains_pointer(rect) {
            let (zoom, sx, shift, sy) = ui.input(|i| (i.zoom_delta(), i.raw_scroll_delta.x, i.modifiers.shift, i.raw_scroll_delta.y));
            if zoom != 1.0 {
                self.pps = (self.pps * zoom).clamp(4.0, 600.0);
            }
            let dx = if shift { sy } else { sx };
            if dx != 0.0 {
                self.scroll = (self.scroll - dx as f64 / pps).max(0.0);
            }
        }
        // follow playhead during playback
        if self.playing {
            let view = (lane_area.width() as f64) / pps;
            if self.playhead > self.scroll + view * 0.95 || self.playhead < self.scroll {
                self.scroll = (self.playhead - view * 0.1).max(0.0);
            }
        }
        let scroll = self.scroll;

        painter.rect_filled(rect, 0.0, Color32::from_rgb(28, 29, 33));

        // ruler
        let ruler = Rect::from_min_max(Pos2::new(lane_left, rect.top()), Pos2::new(rect.right(), rect.top() + RULER_H));
        painter.rect_filled(Rect::from_min_max(rect.min, Pos2::new(rect.right(), rect.top() + RULER_H)), 0.0, Color32::from_rgb(38, 40, 45));
        let steps = [0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0, 600.0];
        let step = steps.iter().copied().find(|s| s * pps >= 80.0).unwrap_or(600.0);
        let first = (scroll / step).floor() * step;
        let mut t = first;
        while tx(t, scroll) < rect.right() {
            let x = tx(t, scroll);
            if x >= lane_left {
                lane_painter.line_segment([Pos2::new(x, ruler.bottom() - 8.0), Pos2::new(x, ruler.bottom())], Stroke::new(1.0, Color32::GRAY));
                lane_painter.text(Pos2::new(x + 3.0, ruler.top() + 8.0), Align2::LEFT_CENTER, fmt_time(t), FontId::monospace(10.0), Color32::GRAY);
            }
            t += step;
        }
        let ruler_resp = ui.interact(ruler, egui::Id::new("ruler"), Sense::click_and_drag());
        if ruler_resp.dragged() || ruler_resp.clicked() {
            if let Some(p) = ruler_resp.interact_pointer_pos() {
                self.stop_playback();
                let raw = (scroll + ((p.x - lane_left) as f64 / pps)).max(0.0);
                // Pull to the nearest clip start/end within PLAYHEAD_SNAP_PX; hold Alt to scrub freely.
                let free = ui.input(|i| i.modifiers.alt);
                let hit = if free { None } else { nearest_within(self.project.edge_points(None), raw, (PLAYHEAD_SNAP_PX / self.pps) as f64) };
                self.playhead = hit.unwrap_or(raw);
                self.snap_marker = hit;
            }
        }
        if ruler_resp.drag_stopped() || ruler_resp.clicked() {
            self.snap_marker = None;
        }
        // in / out range and markers
        if let (Some(a), Some(b)) = (self.project.mark_in, self.project.mark_out) {
            let band = Rect::from_min_max(Pos2::new(tx(a.min(b), scroll).max(lane_left), ruler.bottom() - 6.0), Pos2::new(tx(a.max(b), scroll).max(lane_left), ruler.bottom()));
            lane_painter.rect_filled(band, 0.0, Color32::from_rgba_unmultiplied(90, 170, 255, 150));
        }
        for (label, v) in [("I", self.project.mark_in), ("O", self.project.mark_out)] {
            if let Some(t) = v {
                let x = tx(t, scroll);
                if x >= lane_left && x <= rect.right() {
                    lane_painter.line_segment([Pos2::new(x, ruler.top()), Pos2::new(x, rect.bottom())], Stroke::new(1.0, Color32::from_rgb(90, 170, 255)));
                    lane_painter.text(Pos2::new(x + 2.0, ruler.top() + 20.0), Align2::LEFT_CENTER, label, FontId::proportional(10.0), Color32::from_rgb(120, 190, 255));
                }
            }
        }
        for mk in &self.project.markers {
            let x = tx(mk.t, scroll);
            if x >= lane_left && x <= rect.right() {
                lane_painter.line_segment([Pos2::new(x, ruler.bottom()), Pos2::new(x, rect.bottom())], Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 170, 60, 110)));
                lane_painter.add(egui::Shape::convex_polygon(
                    vec![Pos2::new(x - 5.0, ruler.top() + 1.0), Pos2::new(x + 5.0, ruler.top() + 1.0), Pos2::new(x, ruler.top() + 9.0)],
                    Color32::from_rgb(255, 170, 60),
                    Stroke::NONE,
                ));
                lane_painter.text(Pos2::new(x + 7.0, ruler.top() + 20.0), Align2::LEFT_CENTER, elide(&mk.name, 14), FontId::proportional(10.0), Color32::from_rgb(255, 190, 100));
            }
        }

        // track rows (highest track on top)
        self.track_rects.clear();
        let mut track_toggles: Vec<(usize, bool)> = vec![]; // (track, is_mute)
        for row in 0..n {
            let ti = n - 1 - row;
            let top = rect.top() + RULER_H + row as f32 * TRACK_H;
            let row_rect = Rect::from_min_size(Pos2::new(lane_left, top), Vec2::new(rect.right() - lane_left, TRACK_H));
            self.track_rects.push((ti, row_rect));
            let shade = if row % 2 == 0 { Color32::from_rgb(33, 35, 40) } else { Color32::from_rgb(30, 32, 36) };
            painter.rect_filled(Rect::from_min_size(Pos2::new(rect.left(), top), Vec2::new(rect.width(), TRACK_H)), 0.0, shade);
            painter.line_segment([Pos2::new(rect.left(), top + TRACK_H), Pos2::new(rect.right(), top + TRACK_H)], Stroke::new(1.0, Color32::from_gray(20)));

            // header
            let tr = &self.project.tracks[ti];
            painter.text(Pos2::new(rect.left() + 8.0, top + 16.0), Align2::LEFT_CENTER, &tr.name, FontId::proportional(12.0), Color32::LIGHT_GRAY);
            for (k, (label, on)) in [("M", tr.muted), ("H", tr.hidden)].into_iter().enumerate() {
                let r = Rect::from_min_size(Pos2::new(rect.left() + 8.0 + k as f32 * 30.0, top + 28.0), Vec2::new(26.0, 18.0));
                let resp = ui.interact(r, egui::Id::new(("trk", ti, k)), Sense::click());
                let fill = if on { Color32::from_rgb(170, 70, 60) } else { Color32::from_gray(55) };
                painter.rect_filled(r, 3.0, fill);
                painter.text(r.center(), Align2::CENTER_CENTER, label, FontId::proportional(11.0), Color32::WHITE);
                resp.clone().on_hover_text(if k == 0 { "Mute track audio" } else { "Hide track video" });
                if resp.clicked() {
                    track_toggles.push((ti, k == 0));
                }
            }
        }
        painter.line_segment([Pos2::new(lane_left, rect.top()), Pos2::new(lane_left, rect.bottom())], Stroke::new(1.0, Color32::from_gray(15)));

        // click on empty lane area deselects
        let lanes_rect = Rect::from_min_max(Pos2::new(lane_left, rect.top() + RULER_H), rect.max);
        let lane_resp = ui.interact(lanes_rect, egui::Id::new("lanes"), Sense::click());
        if lane_resp.clicked() {
            self.selected = None;
        }

        // clips
        let snapshot: Vec<Clip> = self.project.clips.clone();
        let pointer = ui.input(|i| i.pointer.interact_pos());
        for c in &snapshot {
            let Some(row) = (0..n).find(|r| n - 1 - r == c.track) else { continue };
            let top = rect.top() + RULER_H + row as f32 * TRACK_H;
            let r = Rect::from_min_max(Pos2::new(tx(c.start, scroll), top + 3.0), Pos2::new(tx(c.end(), scroll), top + TRACK_H - 3.0));
            if r.right() < lane_left || r.left() > rect.right() {
                continue;
            }
            let Some(m) = self.media_for(c) else { continue };
            let sel = self.selected == Some(c.id);
            let base = if c.title.is_some() {
                Color32::from_rgb(120, 85, 175)
            } else {
                match m.kind {
                MediaKind::Video => Color32::from_rgb(52, 105, 170),
                MediaKind::Audio => Color32::from_rgb(55, 140, 95),
                MediaKind::Image if c.freeze => Color32::from_rgb(70, 150, 165),
                MediaKind::Image => Color32::from_rgb(150, 100, 50),
                }
            };
            lane_painter.rect_filled(r, 4.0, base);
            if let Some(tex) = self.thumbs.get(&m.id) {
                if m.kind != MediaKind::Audio {
                    let th = Rect::from_min_size(r.min + Vec2::new(2.0, 2.0), Vec2::new(r.height() * 16.0 / 9.0 - 4.0, r.height() - 4.0));
                    let th = th.intersect(r.shrink(1.0));
                    lane_painter.image(tex.id(), th, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::from_white_alpha(160));
                }
            }
            if m.kind == MediaKind::Audio && m.duration > 0.0 {
                if let Some(tex) = self.ext_wave(m.id) {
                    let a = (c.in_point / m.duration).clamp(0.0, 1.0) as f32;
                    let b = ((c.in_point + c.src_span()) / m.duration).clamp(0.0, 1.0) as f32;
                    let (u0, u1) = if c.reverse { (b, a) } else { (a, b) };
                    let wr = Rect::from_min_max(r.min + Vec2::new(1.0, 14.0), r.max - Vec2::new(1.0, 2.0));
                    lane_painter.image(tex.id(), wr, Rect::from_min_max(Pos2::new(u0, 0.0), Pos2::new(u1, 1.0)), Color32::from_white_alpha(210));
                }
            }
            let label_pos = Pos2::new((r.left().max(lane_left)) + 6.0, r.top() + 12.0);
            let mut label = m.name.clone();
            if (c.speed - 1.0).abs() > 1e-6 {
                label.push_str(&format!("  {}x", (c.speed * 100.0).round() / 100.0));
            }
            if c.reverse {
                label.push_str("  (rev)");
            }
            if c.effects.iter().any(|e| matches!(e, Effect::Zoom { .. })) {
                label.push_str("  [zoom]");
            }
            lane_painter.text(label_pos, Align2::LEFT_CENTER, &label, FontId::proportional(11.0), Color32::WHITE);
            // transition ramps
            for (tr, is_in) in [(c.transition_in, true), (c.transition_out, false)] {
                if tr.kind == TransitionKind::None {
                    continue;
                }
                let w = ((tr.duration as f32) * self.pps).min(r.width() / 2.0);
                let shade = Color32::from_white_alpha(70);
                let pts = if is_in {
                    vec![Pos2::new(r.left(), r.bottom()), Pos2::new(r.left() + w, r.top()), Pos2::new(r.left() + w, r.bottom())]
                } else {
                    vec![Pos2::new(r.right(), r.bottom()), Pos2::new(r.right() - w, r.top()), Pos2::new(r.right() - w, r.bottom())]
                };
                lane_painter.add(egui::Shape::convex_polygon(pts, shade, Stroke::NONE));
            }
            // keyframe markers
            for kt in c.anim.all_times() {
                let x = tx(c.start + kt, scroll);
                let y = r.bottom() - 7.0;
                let d = 4.0;
                lane_painter.add(egui::Shape::convex_polygon(
                    vec![Pos2::new(x, y - d), Pos2::new(x + d, y), Pos2::new(x, y + d), Pos2::new(x - d, y)],
                    Color32::from_rgb(255, 215, 90),
                    Stroke::new(1.0, Color32::BLACK),
                ));
            }
            lane_painter.rect_stroke(r, 4.0, Stroke::new(if sel { 2.0 } else { 1.0 }, if sel { Color32::from_rgb(255, 210, 80) } else { Color32::from_black_alpha(120) }));

            let vis = r.intersect(lanes_rect);
            if vis.width() < 2.0 {
                continue;
            }
            let body = ui.interact(vis, egui::Id::new(("clip", c.id)), Sense::click_and_drag());
            let hw = 7.0_f32.min(vis.width() / 3.0);
            let lh = ui.interact(Rect::from_min_size(r.min, Vec2::new(hw, r.height())).intersect(lanes_rect), egui::Id::new(("clipL", c.id)), Sense::drag());
            let rh = ui.interact(Rect::from_min_max(Pos2::new(r.right() - hw, r.top()), r.max).intersect(lanes_rect), egui::Id::new(("clipR", c.id)), Sense::drag());
            if lh.hovered() || lh.dragged() || rh.hovered() || rh.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }

            if body.clicked() || body.drag_started() || lh.drag_started() || rh.drag_started() {
                self.selected = Some(c.id);
            }
            let kind = if lh.drag_started() {
                Some(DragKind::TrimL)
            } else if rh.drag_started() {
                Some(DragKind::TrimR)
            } else if body.drag_started() {
                Some(DragKind::Move)
            } else {
                None
            };
            if let Some(kind) = kind {
                self.push_undo();
                self.clip_drag = Some(ClipDrag { orig: c.clone(), kind, accum: 0.0 });
            }

            let active = (self.clip_drag.as_ref().map(|d| d.orig.id) == Some(c.id)) && (lh.dragged() || rh.dragged() || body.dragged());
            if active {
                let dx = (lh.drag_delta().x + rh.drag_delta().x + body.drag_delta().x) as f64 / pps;
                self.apply_drag(dx, pointer, &m);
            }
            if lh.drag_stopped() || rh.drag_stopped() || body.drag_stopped() {
                self.finish_drag();
            }
        }

        // drop-target highlight for media dragged from the bin
        if self.drag_media.is_some() {
            if let Some(p) = pointer {
                for (_, r) in &self.track_rects {
                    if r.contains(p) {
                        painter.rect_stroke(r.shrink(1.0), 2.0, Stroke::new(2.0, Color32::from_rgb(255, 210, 80)));
                    }
                }
            }
        }

        // snap indicator while the playhead is pulled onto a clip edge
        if let Some(mt) = self.snap_marker {
            let mx = tx(mt, scroll);
            if mx >= lane_left && mx <= rect.right() {
                let gold = Color32::from_rgb(255, 215, 90);
                painter.line_segment([Pos2::new(mx, rect.top()), Pos2::new(mx, rect.bottom())], Stroke::new(1.0, gold));
                painter.circle_filled(Pos2::new(mx, rect.top() + RULER_H - 6.0), 4.0, gold);
            }
        }

        // playhead
        let px = tx(self.playhead, scroll);
        if px >= lane_left && px <= rect.right() {
            let col = Color32::from_rgb(235, 70, 70);
            painter.line_segment([Pos2::new(px, rect.top()), Pos2::new(px, rect.bottom())], Stroke::new(1.5, col));
            painter.add(egui::Shape::convex_polygon(
                vec![Pos2::new(px - 6.0, rect.top()), Pos2::new(px + 6.0, rect.top()), Pos2::new(px, rect.top() + 10.0)],
                col,
                Stroke::NONE,
            ));
        }

        for (ti, is_mute) in track_toggles {
            self.push_undo();
            let t = &mut self.project.tracks[ti];
            if is_mute {
                t.muted = !t.muted;
            } else {
                t.hidden = !t.hidden;
            }
        }
    }

    fn apply_drag(&mut self, dx: f64, pointer: Option<Pos2>, m: &MediaItem) {
        let Some(drag) = self.clip_drag.as_mut() else { return };
        drag.accum += dx;
        let (orig, kind, acc) = (drag.orig.clone(), drag.kind, drag.accum);
        let is_image = m.kind == MediaKind::Image;
        self.preview_dirty = true;

        match kind {
            DragKind::Move => {
                let raw = (orig.start + acc).max(0.0);
                let start = self.snap(raw, orig.duration, orig.id).max(0.0);
                let track = pointer
                    .and_then(|p| self.track_rects.iter().find(|(_, r)| p.y >= r.top() && p.y < r.bottom()).map(|(t, _)| *t))
                    .unwrap_or(orig.track);
                // Same-track clips never overlap: slide to the nearest free slot. Clips on
                // different tracks may overlap freely (they are layered).
                let start = self.project.nearest_free(track, start, orig.duration, Some(orig.id));
                if let Some(c) = self.project.clip_mut(orig.id) {
                    c.start = start;
                    c.track = track;
                }
            }
            DragKind::TrimL => {
                let snapped = self.snap_edge(orig.start + acc, orig.id);
                let mut delta = snapped - orig.start;
                let sp = orig.speed.max(0.01);
                let mut min_d = if is_image {
                    -orig.start
                } else if orig.reverse {
                    (orig.duration - (m.duration - orig.in_point) / sp).max(-orig.start)
                } else {
                    (-orig.in_point / sp).max(-orig.start)
                };
                // can't trim past the end of the previous clip on this track
                min_d = min_d.max(self.project.gap_before(orig.track, orig.start, Some(orig.id)) - orig.start);
                delta = delta.clamp(min_d, orig.duration - MIN_CLIP);
                if let Some(c) = self.project.clip_mut(orig.id) {
                    c.start = orig.start + delta;
                    c.duration = orig.duration - delta;
                    if !is_image && !orig.reverse {
                        c.in_point = orig.in_point + delta * sp;
                    }
                }
            }
            DragKind::TrimR => {
                let sp = orig.speed.max(0.01);
                let mut max_dur = if is_image {
                    f64::MAX
                } else if orig.reverse {
                    orig.duration + orig.in_point / sp
                } else {
                    (m.duration - orig.in_point) / sp
                };
                // can't extend into the next clip on this track
                max_dur = max_dur.min(self.project.gap_after(orig.track, orig.end(), Some(orig.id)) - orig.start);
                let dur = (self.snap_edge(orig.end() + acc, orig.id) - orig.start).clamp(MIN_CLIP, max_dur.max(MIN_CLIP));
                if let Some(c) = self.project.clip_mut(orig.id) {
                    c.duration = dur;
                    if !is_image && orig.reverse {
                        c.in_point = (orig.in_point - (dur - orig.duration) * sp).max(0.0);
                    }
                }
            }
        }
    }

    fn finish_drag(&mut self) {
        if let Some(d) = self.clip_drag.take() {
            if let Some(c) = self.project.clip(d.orig.id).cloned() {
                if self.project.overlaps(c.track, c.start, c.duration, Some(c.id)) {
                    if let Some(cm) = self.project.clip_mut(c.id) {
                        *cm = d.orig;
                    }
                    self.status = "Clips can't overlap on the same track.".into();
                }
            }
            self.preview_dirty = true;
        }
    }

    fn finish_media_drag(&mut self, ctx: &egui::Context) {
        let Some(mid) = self.drag_media else { return };
        let (released, pos) = ctx.input(|i| (i.pointer.any_released(), i.pointer.interact_pos()));
        if released {
            self.drag_media = None;
            if let Some(p) = pos {
                let hit = self.track_rects.iter().find(|(_, r)| r.contains(p)).map(|(t, _)| *t);
                if let Some(track) = hit {
                    let t = (self.scroll + ((p.x - self.lane_left) as f64 / self.pps as f64)).max(0.0);
                    self.push_undo();
                    if let Some(id) = self.project.add_clip(mid, track, t) {
                        self.selected = Some(id);
                    }
                }
            }
        } else if let (Some(p), Some(m)) = (pos, self.project.media(mid)) {
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("ghost")));
            let r = Rect::from_min_size(p + Vec2::new(10.0, 6.0), Vec2::new(150.0, 22.0));
            painter.rect_filled(r, 4.0, Color32::from_rgba_unmultiplied(70, 110, 170, 220));
            painter.text(r.left_center() + Vec2::new(6.0, 0.0), Align2::LEFT_CENTER, elide(&m.name, 22), FontId::proportional(12.0), Color32::WHITE);
            ctx.request_repaint();
        }
    }

    // ---------------------------------------------------------------- UI: export window

    fn refresh_devices(&mut self) {
        self.audio_devices = record::list_audio_devices();
        self.devices_loaded = true;
        if self.voice_mic.is_none() {
            self.voice_mic = self.audio_devices.first().cloned();
        }
    }

    fn begin_recording(&mut self, ctx: &egui::Context, kind: RecKind) {
        self.rec_msg.clear();
        let built = match kind {
            RecKind::Screen => {
                let mut st = self.rec_screen.clone();
                st.normalize();
                let out = st.output.clone().unwrap_or_else(|| record::default_output("Screen", st.container.ext()));
                record::screen_args(&self.screen_opts, &st, &out).map(|a| (a, out))
            }
            RecKind::Voice => {
                let st = self.rec_voice.clone();
                let ext = record::audio_ext(st.acodec);
                let mut out = st.output.clone().unwrap_or_else(|| record::default_output("Voice", ext));
                out.set_extension(ext);
                record::voice_args(self.voice_mic.as_deref().unwrap_or(""), &st, &out).map(|a| (a, out))
            }
        };
        match built {
            Ok((args, out)) => match Recorder::start(&args, out) {
                Ok(r) => self.recorder = Some((kind, r)),
                Err(e) => {
                    self.rec_msg = e;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                }
            },
            Err(e) => {
                self.rec_msg = e;
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            }
        }
    }

    fn poll_recorder(&mut self, ctx: &egui::Context) {
        if let Some(t) = self.rec_countdown {
            if t.elapsed().as_secs_f32() >= 3.0 {
                self.rec_countdown = None;
                self.begin_recording(ctx, RecKind::Screen);
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
        let mut finished: Option<Result<PathBuf, String>> = None;
        if let Some((_, rec)) = self.recorder.as_mut() {
            match rec.poll() {
                Some(r) => finished = Some(r),
                None => ctx.request_repaint_after(std::time::Duration::from_millis(250)),
            }
        }
        if let Some(res) = finished {
            self.recorder = None;
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            match res {
                Ok(path) => {
                    self.rec_msg = format!("Saved: {}", path.display());
                    if self.rec_add_to_timeline {
                        self.pending_add = Some(path.clone());
                    }
                    self.import_path(ctx, path);
                }
                Err(e) => self.rec_msg = format!("Recording failed:\n{e}"),
            }
        }
    }

    fn record_window(&mut self, ctx: &egui::Context) {
        let Some(kind) = self.rec_open else { return };
        if !self.devices_loaded {
            self.refresh_devices();
        }
        let busy = self.recorder.is_some() || self.rec_countdown.is_some();
        let (mut open, mut start, mut stop, mut refresh) = (true, false, false, false);
        let title = if kind == RecKind::Screen { "Record screen" } else { "Record voice" };

        egui::Window::new(title).open(&mut open).collapsible(false).resizable(false).default_width(460.0).show(ctx, |ui| {
            ui.add_enabled_ui(!busy, |ui| {
                if kind == RecKind::Screen {
                    ui.label(egui::RichText::new("Capture").strong());
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.screen_opts.mode, CaptureMode::Desktop, "Entire desktop");
                        ui.selectable_value(&mut self.screen_opts.mode, CaptureMode::Region, "Region");
                        ui.selectable_value(&mut self.screen_opts.mode, CaptureMode::Window, "Window");
                    });
                    match self.screen_opts.mode {
                        CaptureMode::Region => {
                            ui.horizontal(|ui| {
                                ui.label("X");
                                ui.add(egui::DragValue::new(&mut self.screen_opts.x).range(-10000..=10000));
                                ui.label("Y");
                                ui.add(egui::DragValue::new(&mut self.screen_opts.y).range(-10000..=10000));
                                ui.label("W");
                                ui.add(egui::DragValue::new(&mut self.screen_opts.w).range(16..=16384));
                                ui.label("H");
                                ui.add(egui::DragValue::new(&mut self.screen_opts.h).range(16..=16384));
                            });
                        }
                        CaptureMode::Window => {
                            ui.horizontal(|ui| {
                                ui.label("Window title");
                                ui.add(egui::TextEdit::singleline(&mut self.screen_opts.window_title).desired_width(260.0).hint_text("exact title bar text"));
                            });
                        }
                        CaptureMode::Desktop => {}
                    }
                    ui.checkbox(&mut self.screen_opts.draw_mouse, "Show mouse cursor");
                    ui.add_space(4.0);
                    device_combo(ui, "Microphone", "rec_mic", &mut self.screen_opts.mic, &self.audio_devices);
                    device_combo(ui, "System audio", "rec_sys", &mut self.screen_opts.system_audio, &self.audio_devices);
                    ui.horizontal(|ui| {
                        if ui.small_button("Refresh devices").clicked() {
                            refresh = true;
                        }
                    });
                    ui.label(egui::RichText::new("System audio needs a loopback device such as \"Stereo Mix\" (enable it in Windows Sound settings > Recording).").weak().size(11.0));
                    ui.checkbox(&mut self.rec_minimize, "Minimize ReelForge while recording (restore it from the taskbar to stop)");
                } else {
                    device_combo(ui, "Microphone", "voice_mic", &mut self.voice_mic, &self.audio_devices);
                    if ui.small_button("Refresh devices").clicked() {
                        refresh = true;
                    }
                }
                ui.checkbox(&mut self.rec_add_to_timeline, "Add to timeline when finished");
                ui.separator();
                ui.label(egui::RichText::new("Output").strong());
                if kind == RecKind::Screen {
                    settings_grid(ui, &mut self.rec_screen, GridMode::Screen, (0, 0), 0.0);
                } else {
                    settings_grid(ui, &mut self.rec_voice, GridMode::Voice, (0, 0), 0.0);
                }
            });

            ui.add_space(8.0);
            if let Some(t) = self.rec_countdown {
                ui.label(format!("Starting in {}...", (3.0 - t.elapsed().as_secs_f32()).ceil().max(1.0) as u32));
            } else if let Some((_, rec)) = &self.recorder {
                if rec.stopping() {
                    ui.label("Finalizing file...");
                } else {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("Recording  {}", fmt_time(rec.elapsed()))).color(Color32::from_rgb(230, 70, 70)).strong());
                        if ui.button("  Stop  ").clicked() {
                            stop = true;
                        }
                    });
                }
            } else {
                ui.horizontal(|ui| {
                    if ui.add_enabled(self.ffmpeg_ok, egui::Button::new("  Start recording  ")).clicked() {
                        start = true;
                    }
                    if !self.ffmpeg_ok {
                        ui.label(egui::RichText::new("FFmpeg not found").weak());
                    }
                });
            }
            if !self.rec_msg.is_empty() {
                ui.add_space(4.0);
                ui.label(&self.rec_msg);
            }
        });

        if refresh {
            self.refresh_devices();
        }
        if stop {
            if let Some((_, rec)) = self.recorder.as_mut() {
                rec.request_stop();
            }
        }
        if start {
            if kind == RecKind::Screen {
                self.rec_msg.clear();
                if self.screen_opts.mode == CaptureMode::Window && self.screen_opts.window_title.trim().is_empty() {
                    self.rec_msg = "Enter the exact title of the window to capture.".to_string();
                } else {
                    if self.rec_minimize {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                    }
                    self.rec_countdown = Some(Instant::now());
                }
            } else {
                self.begin_recording(ctx, RecKind::Voice);
            }
        }
        if !open && !busy {
            self.rec_open = None;
        }
    }

    fn close_now(&mut self, ctx: &egui::Context) {
        self.allow_close = true;
        self.close_prompt = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    /// Exit menu item / window close: ask first when there are unsaved changes.
    fn request_exit(&mut self, ctx: &egui::Context) {
        if self.is_dirty() {
            self.close_prompt = true;
        } else {
            self.close_now(ctx);
        }
    }

    fn close_dialog(&mut self, ctx: &egui::Context) {
        if !self.close_prompt {
            return;
        }
        let mut choice: Option<u8> = None; // 0 save, 1 discard, 2 cancel
        egui::Window::new("Unsaved changes").collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.label("This project has unsaved changes.");
            ui.label("Do you want to save it before closing?");
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("  Save  ").clicked() {
                    choice = Some(0);
                }
                if ui.button("  Don't save  ").clicked() {
                    choice = Some(1);
                }
                if ui.button("  Cancel  ").clicked() {
                    choice = Some(2);
                }
            });
        });
        match choice {
            // Only close once the file really was written; a cancelled or failed save keeps the prompt.
            Some(0) => {
                if self.save_project(false) {
                    self.close_now(ctx);
                }
            }
            Some(1) => self.close_now(ctx),
            Some(2) => self.close_prompt = false,
            _ => {}
        }
    }

    fn export_window(&mut self, ctx: &egui::Context) {
        if !self.export_open {
            return;
        }
        let mut open = true;
        let mut start = false;
        let mut cancel = false;
        let mut batch_go: Option<Vec<render::ExportPreset>> = None;
        let rendering = self.render_rx.is_some();
        let (ow, oh) = self.project.output_size(self.settings.short_side);
        let dur = self.project.duration();

        egui::Window::new("Export video").open(&mut open).collapsible(false).resizable(false).default_width(420.0).show(ctx, |ui| {
            ui.add_enabled_ui(!rendering, |ui| {
                settings_grid(ui, &mut self.settings, GridMode::Export, (ow, oh), dur);
                ui.add_space(6.0);
                ui.separator();
                ui.label(egui::RichText::new("Presets").strong());
                let mut presets = render::ExportPreset::builtin();
                let n_builtin = presets.len();
                presets.extend(self.ext.cfg.export_presets.iter().cloned());
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("exp_preset").selected_text("Apply a preset...").show_ui(ui, |ui| {
                        for p in &presets {
                            if ui.selectable_label(false, &p.name).clicked() {
                                p.apply(&mut self.settings);
                            }
                        }
                    });
                    ui.add(egui::TextEdit::singleline(&mut self.ext.preset_name).hint_text("name").desired_width(110.0));
                    if ui.add_enabled(!self.ext.preset_name.trim().is_empty(), egui::Button::new("Save as preset")).clicked() {
                        let np = render::ExportPreset::from_settings(self.ext.preset_name.trim(), &self.settings);
                        self.ext.cfg.export_presets.retain(|p| p.name != np.name);
                        self.ext.cfg.export_presets.push(np);
                        self.ext.save_cfg();
                    }
                });
                egui::CollapsingHeader::new("Batch export (several presets at once)").id_salt("batch").show(ui, |ui| {
                    for (i, p) in presets.iter().enumerate() {
                        ui.horizontal(|ui| {
                            let mut on = self.batch_sel.contains(&p.name);
                            if ui.checkbox(&mut on, &p.name).changed() {
                                if on {
                                    self.batch_sel.insert(p.name.clone());
                                } else {
                                    self.batch_sel.remove(&p.name);
                                }
                            }
                            if i >= n_builtin && ui.small_button("Delete").clicked() {
                                let name = p.name.clone();
                                self.ext.cfg.export_presets.retain(|x| x.name != name);
                                self.batch_sel.remove(&name);
                                self.ext.save_cfg();
                            }
                        });
                    }
                    ui.horizontal(|ui| {
                        ui.label(self.batch_dir.as_ref().map(|d| elide(&d.to_string_lossy(), 30)).unwrap_or_else(|| "(no folder)".into()));
                        if ui.button("Folder...").clicked() {
                            if let Some(d) = rfd::FileDialog::new().pick_folder() {
                                self.batch_dir = Some(d);
                            }
                        }
                    });
                    let chosen: Vec<render::ExportPreset> = presets.iter().filter(|p| self.batch_sel.contains(&p.name)).cloned().collect();
                    let ok = !chosen.is_empty() && self.batch_dir.is_some() && dur > 0.0 && self.ffmpeg_ok;
                    if ui.add_enabled(ok, egui::Button::new(format!("Export {} file(s)", chosen.len()))).clicked() {
                        batch_go = Some(chosen);
                    }
                    ui.label(egui::RichText::new("Files are named after the project and preset. Uses the subtitle and range options below. Presets set the size, codec, bitrate and loudness.").weak().small());
                });
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Audio, subtitles and range").strong());
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.settings.loudnorm, "Normalize loudness to");
                    ui.add_enabled(self.settings.loudnorm, egui::DragValue::new(&mut self.settings.lufs).range(-30.0..=-6.0).speed(0.1).suffix(" LUFS"));
                });
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(!self.project.subtitles.is_empty(), |ui| {
                        ui.checkbox(&mut self.settings.burn_subs, "Burn subtitles into picture");
                        ui.checkbox(&mut self.settings.sidecar_srt, "Also write .srt");
                    });
                });
                let has_range = matches!((self.project.mark_in, self.project.mark_out), (Some(a), Some(b)) if (b - a).abs() > 0.05);
                ui.add_enabled(has_range, egui::Checkbox::new(&mut self.settings.use_range, "Export only the in-out range"));
            });

            ui.add_space(10.0);
            if rendering {
                if self.batch_total > 0 {
                    ui.label(format!("Batch: file {} of {}", self.batch_total - self.batch.len(), self.batch_total));
                }
                ui.add(egui::ProgressBar::new(self.render_progress).show_percentage().animate(true));
                if ui.button("Cancel export").clicked() {
                    cancel = true;
                }
            } else {
                if !self.render_msg.is_empty() {
                    ui.label(&self.render_msg);
                    ui.add_space(6.0);
                }
                let ready = self.settings.output.is_some() && dur > 0.0 && self.ffmpeg_ok;
                ui.horizontal(|ui| {
                    if ui.add_enabled(ready, egui::Button::new("  Start export  ")).clicked() {
                        start = true;
                    }
                    if !ready {
                        let why = if !self.ffmpeg_ok {
                            "FFmpeg not found"
                        } else if dur <= 0.0 {
                            "Timeline is empty"
                        } else {
                            "Choose an output file"
                        };
                        ui.label(egui::RichText::new(why).weak());
                    }
                });
            }
        });

        if cancel {
            self.batch.clear();
            self.render_cancel.store(true, Ordering::Relaxed);
        }
        if let (Some(list), Some(dir)) = (batch_go, self.batch_dir.clone()) {
            self.start_batch(ctx, dir, list);
        }
        if start {
            self.render_progress = 0.0;
            self.render_msg.clear();
            self.batch_total = 0;
            let st = self.settings.clone();
            self.begin_render(ctx, st);
        }
        if !open && !rendering {
            self.export_open = false;
        }
    }
}

fn device_combo(ui: &mut egui::Ui, label: &str, salt: &str, sel: &mut Option<String>, devices: &[String]) {
    ui.horizontal(|ui| {
        ui.label(label);
        let text = sel.clone().unwrap_or_else(|| "None".to_string());
        egui::ComboBox::from_id_salt(salt).width(280.0).selected_text(elide(&text, 36)).show_ui(ui, |ui| {
            if ui.selectable_label(sel.is_none(), "None").clicked() {
                *sel = None;
            }
            for d in devices {
                if ui.selectable_label(sel.as_deref() == Some(d.as_str()), d).clicked() {
                    *sel = Some(d.clone());
                }
            }
        });
    });
}

/// The encoding options shared by Export, Record screen and Record voice.
fn settings_grid(ui: &mut egui::Ui, s: &mut RenderSettings, mode: GridMode, dims: (u32, u32), dur: f64) {
    let id = match mode {
        GridMode::Export => "g_exp",
        GridMode::Screen => "g_scr",
        GridMode::Voice => "g_voi",
    };
    ui.push_id(id, |ui| {
        egui::Grid::new("grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label("Output file");
            ui.horizontal(|ui| {
                let txt = s.output.as_ref().map(|p| elide(&p.to_string_lossy(), 30)).unwrap_or_else(|| {
                    if mode == GridMode::Export {
                        "(not chosen)".to_string()
                    } else {
                        "(auto-named in Videos)".to_string()
                    }
                });
                ui.label(txt);
                if ui.button("Browse...").clicked() {
                    let (ext, label) = if mode == GridMode::Voice { (record::audio_ext(s.acodec), "Audio") } else { (s.container.ext(), s.container.label()) };
                    let stem = match mode {
                        GridMode::Export => "export",
                        GridMode::Screen => "screen recording",
                        GridMode::Voice => "voice recording",
                    };
                    if let Some(p) = rfd::FileDialog::new().add_filter(label, &[ext]).set_file_name(format!("{stem}.{ext}")).save_file() {
                        s.output = Some(p);
                    }
                }
                if mode != GridMode::Export && s.output.is_some() && ui.small_button("Auto").clicked() {
                    s.output = None;
                }
            });
            ui.end_row();

            if mode != GridMode::Voice {
                ui.label("Format");
                egui::ComboBox::from_id_salt("cont").selected_text(s.container.label()).show_ui(ui, |ui| {
                    for c in Container::ALL {
                        if ui.selectable_value(&mut s.container, c, c.label()).clicked() {
                            s.normalize();
                            if let Some(p) = &mut s.output {
                                p.set_extension(c.ext());
                            }
                        }
                    }
                });
                ui.end_row();

                ui.label("Resolution");
                let res_label = RESOLUTIONS.iter().find(|r| r.1 == s.short_side).map(|r| r.0).unwrap_or(if s.short_side == 0 { "Native (capture size)" } else { "Custom" });
                let shown = if mode == GridMode::Export { format!("{res_label}  ({} x {})", dims.0, dims.1) } else { res_label.to_string() };
                egui::ComboBox::from_id_salt("res").selected_text(shown).show_ui(ui, |ui| {
                    if mode == GridMode::Screen {
                        ui.selectable_value(&mut s.short_side, 0, "Native (capture size)");
                    }
                    for (label, v) in RESOLUTIONS {
                        if ui.selectable_value(&mut s.short_side, v, label).clicked() {
                            s.video_kbps = render::suggested_bitrate_kbps(v);
                        }
                    }
                });
                ui.end_row();

                ui.label("Frame rate");
                egui::ComboBox::from_id_salt("rfps").selected_text(format!("{} fps", s.fps)).show_ui(ui, |ui| {
                    for f in [24, 25, 30, 50, 60] {
                        ui.selectable_value(&mut s.fps, f, format!("{f} fps"));
                    }
                });
                ui.end_row();

                ui.label("Video codec");
                egui::ComboBox::from_id_salt("vc").selected_text(s.vcodec.label()).show_ui(ui, |ui| {
                    for &c in s.container.video_codecs() {
                        ui.selectable_value(&mut s.vcodec, c, c.label());
                    }
                });
                ui.end_row();

                if s.vcodec.uses_bitrate() {
                    ui.label("Video quality");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut s.rate_control, RateControl::Bitrate, "Bitrate");
                        if s.vcodec.supports_crf() {
                            ui.selectable_value(&mut s.rate_control, RateControl::Crf, "Constant quality");
                        } else {
                            s.rate_control = RateControl::Bitrate;
                        }
                    });
                    ui.end_row();

                    if s.rate_control == RateControl::Bitrate || !s.vcodec.supports_crf() {
                        ui.label("Video bitrate");
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut s.video_kbps).range(250..=200_000).speed(50.0).suffix(" kbps"));
                            ui.label(egui::RichText::new(format!("({:.1} Mbps)", s.video_kbps as f32 / 1000.0)).weak());
                        });
                    } else {
                        ui.label("CRF (lower = better)");
                        ui.add(egui::Slider::new(&mut s.crf, 10..=40));
                    }
                    ui.end_row();
                }

                ui.label("Encoding speed");
                egui::ComboBox::from_id_salt("spd").selected_text(s.speed.label()).show_ui(ui, |ui| {
                    for sp in Speed::ALL {
                        ui.selectable_value(&mut s.speed, sp, sp.label());
                    }
                });
                ui.end_row();
            }

            ui.label("Audio codec");
            let codecs: Vec<AudioCodec> = if mode == GridMode::Voice { AudioCodec::ALL.to_vec() } else { s.container.audio_codecs().to_vec() };
            egui::ComboBox::from_id_salt("ac").selected_text(s.acodec.label()).show_ui(ui, |ui| {
                for c in codecs {
                    if ui.selectable_value(&mut s.acodec, c, c.label()).clicked() && mode == GridMode::Voice {
                        if let Some(p) = &mut s.output {
                            p.set_extension(record::audio_ext(c));
                        }
                    }
                }
            });
            ui.end_row();

            if s.acodec.uses_bitrate() {
                ui.label("Audio bitrate");
                egui::ComboBox::from_id_salt("ab").selected_text(format!("{} kbps", s.audio_kbps)).show_ui(ui, |ui| {
                    for b in [64, 96, 128, 160, 192, 256, 320, 384, 512] {
                        ui.selectable_value(&mut s.audio_kbps, b, format!("{b} kbps"));
                    }
                });
                ui.end_row();
            }
        });

        if mode == GridMode::Export {
            let vk = if s.vcodec.uses_bitrate() && s.rate_control == RateControl::Bitrate { s.video_kbps as f64 } else { 0.0 };
            let ak = if s.acodec.uses_bitrate() { s.audio_kbps as f64 } else { 0.0 };
            if vk > 0.0 {
                ui.add_space(6.0);
                ui.label(egui::RichText::new(format!("Estimated size: ~{:.1} MB for {}", (vk + ak) * dur / 8.0 / 1000.0, fmt_time(dur))).weak());
            }
        }
        if mode == GridMode::Screen {
            ui.add_space(4.0);
            ui.label(egui::RichText::new("Tip: keep encoding speed on \"Fast\" so the encoder keeps up in real time. Recordings never scale up beyond the screen size.").weak().size(11.0));
        }
    });
}

fn title_editor(ui: &mut egui::Ui, t: &mut TitleSpec) {
    ui.add(egui::TextEdit::multiline(&mut t.text).desired_rows(3).desired_width(f32::INFINITY));
    ui.add_space(4.0);
    egui::ComboBox::from_id_salt("title_font").selected_text(t.font.label()).show_ui(ui, |ui| {
        for f in TitleFont::ALL {
            ui.selectable_value(&mut t.font, f, f.label());
        }
    });
    ui.add(egui::Slider::new(&mut t.size, 2.0..=25.0).text("Size %"));
    ui.horizontal(|ui| {
        ui.label("Color");
        ui.color_edit_button_srgb(&mut t.color);
    });
    egui::ComboBox::from_id_salt("title_pos").selected_text(if t.pos.is_some() { "Custom (dragged)" } else { t.position.label() }).show_ui(ui, |ui| {
        for p in TitlePos::ALL {
            if ui.selectable_value(&mut t.position, p, p.label()).clicked() {
                t.pos = None;
            }
        }
    });
    ui.label(egui::RichText::new("Tip: drag the title in the preview to place it.").weak().size(11.0));
    ui.checkbox(&mut t.background, "Background box");
    if t.background {
        ui.horizontal(|ui| {
            ui.label("Color");
            ui.color_edit_button_srgb(&mut t.bg_color);
            for (name, c) in [("Black", [0u8, 0, 0]), ("White", [255, 255, 255]), ("Red", [200, 40, 40]), ("Blue", [30, 80, 200]), ("Green", [30, 150, 80]), ("Yellow", [240, 200, 40])] {
                let b = egui::Button::new("   ").fill(Color32::from_rgb(c[0], c[1], c[2]));
                if ui.add(b).on_hover_text(name).clicked() {
                    t.bg_color = c;
                }
            }
        });
        ui.add(egui::Slider::new(&mut t.bg_opacity, 0.0..=1.0).text("Opacity").custom_formatter(|v, _| format!("{:.0}%", v * 100.0)));
    }
    let pct = |v: f64, _: std::ops::RangeInclusive<usize>| if v <= 0.0 { "auto".to_string() } else { format!("{:.0}%", v * 100.0) };
    ui.add(egui::Slider::new(&mut t.box_w, 0.0..=1.0).text("Box width").custom_formatter(pct));
    ui.add(egui::Slider::new(&mut t.box_h, 0.0..=1.0).text("Box height").custom_formatter(pct));
    if (t.box_w > 0.0 || t.box_h > 0.0) && ui.button("Fit box to text").clicked() {
        t.box_w = 0.0;
        t.box_h = 0.0;
    }
}

fn transition_editor(ui: &mut egui::Ui, label: &str, tr: &mut Transition, max_d: f64, salt: &str) {
    ui.horizontal(|ui| {
        ui.label(label);
        egui::ComboBox::from_id_salt(salt).selected_text(tr.kind.label()).show_ui(ui, |ui| {
            for k in TransitionKind::ALL {
                ui.selectable_value(&mut tr.kind, k, k.label());
            }
        });
    });
    if tr.kind != TransitionKind::None {
        let hi = max_d.max(tr.duration).max(0.2);
        ui.add(egui::Slider::new(&mut tr.duration, 0.1..=hi).suffix(" s"));
    }
}

fn effects_editor(ui: &mut egui::Ui, effects: &mut Vec<Effect>) {
    let mut remove: Option<usize> = None;
    for (i, e) in effects.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(e.label()).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("Remove").clicked() {
                    remove = Some(i);
                }
            });
        });
        match e {
            Effect::ColorAdjust { brightness, contrast, saturation } => {
                ui.add(egui::Slider::new(brightness, -1.0..=1.0).text("Brightness"));
                ui.add(egui::Slider::new(contrast, 0.0..=3.0).text("Contrast"));
                ui.add(egui::Slider::new(saturation, 0.0..=3.0).text("Saturation"));
            }
            Effect::Blur { sigma } => {
                ui.add(egui::Slider::new(sigma, 0.5..=40.0).text("Amount"));
            }
            Effect::Sharpen { amount } => {
                ui.add(egui::Slider::new(amount, -2.0..=5.0).text("Amount"));
            }
            Effect::ChromaKey { color, similarity, blend } => {
                ui.horizontal(|ui| {
                    ui.label("Color to remove");
                    ui.color_edit_button_srgb(color);
                });
                ui.add(egui::Slider::new(similarity, 0.01..=0.5).text("Similarity"));
                ui.add(egui::Slider::new(blend, 0.0..=0.5).text("Edge blend"));
            }
            Effect::Zoom { from, to, fx, fy, ease } => {
                ui.add(egui::Slider::new(from, 0.25..=4.0).suffix("x").text("Start zoom"));
                ui.add(egui::Slider::new(to, 0.25..=4.0).suffix("x").text("End zoom"));
                ui.add(egui::Slider::new(fx, 0.0..=1.0).text("Focus X"));
                ui.add(egui::Slider::new(fy, 0.0..=1.0).text("Focus Y"));
                ui.checkbox(ease, "Smooth start / end");
                ui.label(egui::RichText::new("Zooms steadily over the whole clip. The focus point stays fixed on screen.").weak().small());
            }
            Effect::ColorWheels { lift, gamma, gain } => {
                ui.label(egui::RichText::new("R / G / B offsets").weak().small());
                rgb_row(ui, "Lift (shadows)", lift);
                rgb_row(ui, "Gamma (mids)", gamma);
                rgb_row(ui, "Gain (highlights)", gain);
            }
            Effect::Temperature { kelvin, tint } => {
                ui.add(egui::Slider::new(kelvin, 2000.0..=12000.0).suffix(" K").text("Temperature"));
                ui.add(egui::Slider::new(tint, -1.0..=1.0).text("Tint (green - magenta)"));
            }
            Effect::Lut { path } => {
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(path).desired_width(120.0).hint_text("LUT file"));
                    if ui.button("Browse...").clicked() {
                        if let Some(p) = rfd::FileDialog::new().add_filter("3D LUT", &["cube", "3dl", "dat", "m3d", "csp"]).pick_file() {
                            *path = p.to_string_lossy().into_owned();
                        }
                    }
                });
            }
            Effect::HslAdjust { band, hue, sat, intensity } => {
                egui::ComboBox::from_id_salt(("hslband", i)).selected_text(band.label()).show_ui(ui, |ui| {
                    for b in HslBand::ALL {
                        ui.selectable_value(band, b, b.label());
                    }
                });
                ui.add(egui::Slider::new(hue, -180.0..=180.0).suffix(" deg").text("Hue"));
                ui.add(egui::Slider::new(sat, -1.0..=1.0).text("Saturation"));
                ui.add(egui::Slider::new(intensity, -1.0..=1.0).text("Intensity"));
            }
            Effect::Curves { preset } => {
                egui::ComboBox::from_id_salt(("curves", i)).selected_text(preset.label()).show_ui(ui, |ui| {
                    for p in CurvePreset::ALL {
                        ui.selectable_value(preset, p, p.label());
                    }
                });
            }
            Effect::Glow { radius, strength } => {
                ui.add(egui::Slider::new(radius, 2.0..=100.0).text("Radius"));
                ui.add(egui::Slider::new(strength, 0.0..=1.0).text("Strength"));
            }
            Effect::Distort { amount } => {
                ui.add(egui::Slider::new(amount, -1.0..=1.0).text("Amount (+ barrel / - pincushion)"));
            }
            Effect::Rotate { degrees } => {
                ui.add(egui::Slider::new(degrees, -180.0..=180.0).suffix(" deg").text("Angle"));
            }
            Effect::Pixelate { block } => {
                ui.add(egui::Slider::new(block, 2.0..=80.0).text("Block size"));
            }
            Effect::Grain { amount } => {
                ui.add(egui::Slider::new(amount, 0.0..=60.0).text("Amount"));
            }
            Effect::Denoise { strength } => {
                ui.add(egui::Slider::new(strength, 0.0..=1.0).text("Strength"));
            }
            Effect::Stabilize { strength } => {
                ui.add(egui::Slider::new(strength, 0.0..=1.0).text("Strength"));
                ui.label(egui::RichText::new("Smooths camera shake (needs an FFmpeg build with vidstab).").weak().small());
            }
            Effect::Mask { ellipse, cx, cy, w, h, feather, invert } => {
                ui.checkbox(ellipse, "Ellipse (off = rectangle)");
                ui.add(egui::Slider::new(cx, 0.0..=1.0).text("Centre X"));
                ui.add(egui::Slider::new(cy, 0.0..=1.0).text("Centre Y"));
                ui.add(egui::Slider::new(w, 0.05..=1.0).text("Width"));
                ui.add(egui::Slider::new(h, 0.05..=1.0).text("Height"));
                ui.add(egui::Slider::new(feather, 0.0..=0.5).text("Feather"));
                ui.checkbox(invert, "Invert mask");
            }
            _ => {}
        }
        ui.separator();
    }
    if let Some(i) = remove {
        effects.remove(i);
    }
    ui.menu_button("+ Add effect", |ui| {
        for e in Effect::catalog() {
            if ui.button(e.label()).clicked() {
                effects.push(e);
                ui.close_menu();
            }
        }
    });
}

fn audio_fx_editor(ui: &mut egui::Ui, a: &mut AudioFx) {
    ui.add(egui::Slider::new(&mut a.pan, -1.0..=1.0).text("Pan"));
    ui.label(egui::RichText::new("Equalizer").strong());
    ui.add(egui::Slider::new(&mut a.eq_low, -12.0..=12.0).suffix(" dB").text("Low"));
    ui.add(egui::Slider::new(&mut a.eq_mid, -12.0..=12.0).suffix(" dB").text("Mid"));
    ui.add(egui::Slider::new(&mut a.eq_high, -12.0..=12.0).suffix(" dB").text("High"));
    ui.add(egui::Slider::new(&mut a.compress, 0.0..=1.0).text("Compressor"));
    ui.add(egui::Slider::new(&mut a.denoise, 0.0..=1.0).text("Noise reduction"));
    ui.add(egui::Slider::new(&mut a.deess, 0.0..=1.0).text("De-esser"));
    ui.add(egui::Slider::new(&mut a.reverb, 0.0..=1.0).text("Reverb"));
    ui.add(egui::Slider::new(&mut a.echo_ms, 0.0..=1000.0).suffix(" ms").text("Echo delay"));
    ui.add(egui::Slider::new(&mut a.echo_decay, 0.0..=0.9).text("Echo feedback"));
    ui.checkbox(&mut a.normalize, "Normalize loudness");
    if ui.small_button("Reset audio effects").clicked() {
        *a = AudioFx::default();
    }
}

fn rgb_row(ui: &mut egui::Ui, label: &str, v: &mut [f32; 3]) {
    ui.horizontal(|ui| {
        ui.label(label);
        for x in v.iter_mut() {
            ui.add(egui::DragValue::new(x).speed(0.005).range(-1.0..=1.0).fixed_decimals(2));
        }
    });
}

fn anim_editor(ui: &mut egui::Ui, anim: &mut Anim, rel_t: f64, dur: f64) {
    for p in AnimProp::ALL {
        let n = anim.track(p).len();
        egui::CollapsingHeader::new(format!("{} ({n})", p.label())).id_salt(("anim", p.label())).default_open(n > 0).show(ui, |ui| {
            let kfs = anim.track_mut(p);
            let mut remove: Option<usize> = None;
            for (i, k) in kfs.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut k.t).range(0.0..=dur).speed(0.02).fixed_decimals(2).suffix(" s"));
                    ui.add(egui::DragValue::new(&mut k.v).range(p.range()).speed(0.01).fixed_decimals(2));
                    egui::ComboBox::from_id_salt(("ease", i)).width(64.0).selected_text(k.ease.label()).show_ui(ui, |ui| {
                        for e in Ease::ALL {
                            ui.selectable_value(&mut k.ease, e, e.label());
                        }
                    });
                    if ui.small_button("Remove").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                kfs.remove(i);
            }
            kfs.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal));
            if ui.button("+ Keyframe at playhead").clicked() {
                let t = rel_t.clamp(0.0, dur);
                let v = sample_keyframes(kfs, t, p.default_value());
                if let Some(k) = kfs.iter_mut().find(|k| (k.t - t).abs() < 0.02) {
                    k.v = v;
                } else {
                    kfs.push(Keyframe::new(t, v));
                    kfs.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal));
                }
            }
        });
    }
    ui.label(egui::RichText::new("Move the playhead over the clip, then add a keyframe and set its value. Values blend linearly between keyframes.").weak().size(11.0));
}

fn elide(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max {
        s.to_string()
    } else {
        let tail: String = s.chars().skip(n - (max - 1)).collect();
        format!("…{tail}")
    }
}

impl eframe::App for ReelForge {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Window close button / Alt+F4: intercept when there is unsaved work.
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close && self.is_dirty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_prompt = true;
        }
        let name = self.project_path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Untitled".to_string());
        let title = format!("{}{} - ReelForge", name, if self.is_dirty() { " *" } else { "" });
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }

        self.poll(ctx);
        self.poll_recorder(ctx);
        self.ext_poll(ctx);
        self.handle_drops(ctx);
        self.handle_keys(ctx);
        self.advance_playback(ctx);
        self.pump_preview();

        self.menu_bar(ctx);
        self.timeline_panel(ctx);
        if self.ext.cfg.show_media {
            self.media_panel(ctx);
        }
        if self.ext.cfg.show_props {
            self.props_panel(ctx);
        }
        self.preview_panel(ctx);
        self.export_window(ctx);
        self.ext_windows(ctx);
        self.record_window(ctx);
        self.close_dialog(ctx);
        self.finish_media_drag(ctx);

        if self.preview_inflight || self.render_rx.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }
}
