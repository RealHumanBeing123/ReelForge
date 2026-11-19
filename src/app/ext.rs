//! Professional editing extras: commands with customisable shortcuts and macros, in/out points,
//! markers, ripple / lift / extract / insert / overwrite edits, source monitor, video scopes,
//! subtitles, loudness meter, proxies, audio waveforms, effect presets, title templates,
//! workspaces and export presets.
//!
//! This is a child module of `app`, so it can use the editor's private state directly.
use super::*;
use crate::util::command;
use crate::{interchange, render::Loudness, scopes};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

// ------------------------------------------------------------------------- commands & shortcuts

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Cmd {
    PlayPause,
    Split,
    Freeze,
    Delete,
    RippleDelete,
    Undo,
    Redo,
    Save,
    Open,
    Import,
    Export,
    StepBack,
    StepFwd,
    GoStart,
    GoEnd,
    MarkIn,
    MarkOut,
    ClearInOut,
    AddMarker,
    NextMarker,
    PrevMarker,
    Lift,
    Extract,
    ExtendEdit,
    InsertAtPlayhead,
    OverwriteAtPlayhead,
    AddTitle,
    ShuttleSlower,
    ShuttlePause,
    ShuttleFaster,
    SourceMonitor,
    Scopes,
}

impl Cmd {
    pub const ALL: [Cmd; 32] = [
        Cmd::PlayPause,
        Cmd::Split,
        Cmd::Freeze,
        Cmd::Delete,
        Cmd::RippleDelete,
        Cmd::Undo,
        Cmd::Redo,
        Cmd::Save,
        Cmd::Open,
        Cmd::Import,
        Cmd::Export,
        Cmd::StepBack,
        Cmd::StepFwd,
        Cmd::GoStart,
        Cmd::GoEnd,
        Cmd::MarkIn,
        Cmd::MarkOut,
        Cmd::ClearInOut,
        Cmd::AddMarker,
        Cmd::NextMarker,
        Cmd::PrevMarker,
        Cmd::Lift,
        Cmd::Extract,
        Cmd::ExtendEdit,
        Cmd::InsertAtPlayhead,
        Cmd::OverwriteAtPlayhead,
        Cmd::AddTitle,
        Cmd::ShuttleSlower,
        Cmd::ShuttlePause,
        Cmd::ShuttleFaster,
        Cmd::SourceMonitor,
        Cmd::Scopes,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Cmd::PlayPause => "Play / pause",
            Cmd::Split => "Split at playhead",
            Cmd::Freeze => "Freeze frame at playhead",
            Cmd::Delete => "Delete clip",
            Cmd::RippleDelete => "Ripple delete clip",
            Cmd::Undo => "Undo",
            Cmd::Redo => "Redo",
            Cmd::Save => "Save project",
            Cmd::Open => "Open project",
            Cmd::Import => "Import media",
            Cmd::Export => "Export",
            Cmd::StepBack => "Step back one frame",
            Cmd::StepFwd => "Step forward one frame",
            Cmd::GoStart => "Go to start",
            Cmd::GoEnd => "Go to end",
            Cmd::MarkIn => "Set in point",
            Cmd::MarkOut => "Set out point",
            Cmd::ClearInOut => "Clear in / out",
            Cmd::AddMarker => "Add marker",
            Cmd::NextMarker => "Next marker",
            Cmd::PrevMarker => "Previous marker",
            Cmd::Lift => "Lift (remove in-out, leave gap)",
            Cmd::Extract => "Extract (remove in-out, close gap)",
            Cmd::ExtendEdit => "Extend edit to playhead",
            Cmd::InsertAtPlayhead => "Insert media at playhead",
            Cmd::OverwriteAtPlayhead => "Overwrite media at playhead",
            Cmd::AddTitle => "Add title",
            Cmd::ShuttleSlower => "Shuttle slower / back 1 s (J)",
            Cmd::ShuttlePause => "Shuttle pause (K)",
            Cmd::ShuttleFaster => "Shuttle faster (L)",
            Cmd::SourceMonitor => "Source monitor",
            Cmd::Scopes => "Video scopes",
        }
    }

    fn default_shortcut(self) -> Shortcut {
        use egui::Key as K;
        let s = |key, ctrl, shift, alt| Shortcut { key, ctrl, shift, alt };
        match self {
            Cmd::PlayPause => s(K::Space, false, false, false),
            Cmd::Split => s(K::S, false, false, false),
            Cmd::Freeze => s(K::F, false, false, false),
            Cmd::Delete => s(K::Delete, false, false, false),
            Cmd::RippleDelete => s(K::Delete, false, true, false),
            Cmd::Undo => s(K::Z, true, false, false),
            Cmd::Redo => s(K::Z, true, true, false),
            Cmd::Save => s(K::S, true, false, false),
            Cmd::Open => s(K::O, true, false, false),
            Cmd::Import => s(K::I, true, false, false),
            Cmd::Export => s(K::E, true, false, false),
            Cmd::StepBack => s(K::ArrowLeft, false, false, false),
            Cmd::StepFwd => s(K::ArrowRight, false, false, false),
            Cmd::GoStart => s(K::Home, false, false, false),
            Cmd::GoEnd => s(K::End, false, false, false),
            Cmd::MarkIn => s(K::I, false, false, false),
            Cmd::MarkOut => s(K::O, false, false, false),
            Cmd::ClearInOut => s(K::X, false, false, true),
            Cmd::AddMarker => s(K::M, false, false, false),
            Cmd::NextMarker => s(K::ArrowRight, false, true, false),
            Cmd::PrevMarker => s(K::ArrowLeft, false, true, false),
            Cmd::Lift => s(K::Q, false, false, false),
            Cmd::Extract => s(K::W, false, false, false),
            Cmd::ExtendEdit => s(K::E, false, false, false),
            Cmd::InsertAtPlayhead => s(K::Comma, false, false, false),
            Cmd::OverwriteAtPlayhead => s(K::Period, false, false, false),
            Cmd::AddTitle => s(K::T, false, false, false),
            Cmd::ShuttleSlower => s(K::J, false, false, false),
            Cmd::ShuttlePause => s(K::K, false, false, false),
            Cmd::ShuttleFaster => s(K::L, false, false, false),
            Cmd::SourceMonitor => s(K::Y, false, false, false),
            Cmd::Scopes => s(K::U, false, false, false),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Shortcut {
    pub key: egui::Key,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

impl Shortcut {
    pub fn text(&self) -> String {
        let mut s = String::new();
        if self.ctrl {
            s.push_str("Ctrl+");
        }
        if self.shift {
            s.push_str("Shift+");
        }
        if self.alt {
            s.push_str("Alt+");
        }
        s.push_str(self.key.name());
        s
    }

    pub fn parse(text: &str) -> Option<Shortcut> {
        let (mut ctrl, mut shift, mut alt, mut key) = (false, false, false, None);
        for part in text.split('+') {
            match part.trim() {
                "Ctrl" => ctrl = true,
                "Shift" => shift = true,
                "Alt" => alt = true,
                k => key = egui::Key::from_name(k),
            }
        }
        key.map(|key| Shortcut { key, ctrl, shift, alt })
    }

    fn mods(&self) -> egui::Modifiers {
        egui::Modifiers { alt: self.alt, ctrl: self.ctrl, shift: self.shift, mac_cmd: false, command: self.ctrl }
    }

    fn weight(&self) -> u8 {
        self.ctrl as u8 + self.shift as u8 + self.alt as u8
    }
}

// ------------------------------------------------------------------------- saved settings

#[derive(Clone, Serialize, Deserialize)]
pub struct Macro {
    pub name: String,
    pub steps: Vec<Cmd>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct FxPreset {
    pub name: String,
    pub effects: Vec<Effect>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    /// Command name -> shortcut text. Missing entries use the default.
    #[serde(default)]
    pub shortcuts: HashMap<String, String>,
    #[serde(default)]
    pub macros: Vec<Macro>,
    #[serde(default)]
    pub fx_presets: Vec<FxPreset>,
    #[serde(default)]
    pub export_presets: Vec<crate::render::ExportPreset>,
    /// Measured render speed (output pixels x frames per second) from the last export.
    #[serde(default)]
    pub render_rate: f64,
    #[serde(default = "yes")]
    pub show_media: bool,
    #[serde(default = "yes")]
    pub show_props: bool,
    #[serde(default)]
    pub watch_on: bool,
    #[serde(default)]
    pub watch_media: String,
    #[serde(default)]
    pub watch_render: String,
    #[serde(default)]
    pub watch_preset: String,
    #[serde(default)]
    pub farm_dir: String,
    #[serde(default)]
    pub farm_worker: bool,
    #[serde(default)]
    pub raw_cmd: String,
    #[serde(default)]
    pub raw_ext: String,
}

fn yes() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self { shortcuts: HashMap::new(), macros: vec![], fx_presets: vec![], export_presets: vec![], render_rate: 0.0, show_media: true, show_props: true, watch_on: false, watch_media: String::new(), watch_render: String::new(), watch_preset: String::new(), farm_dir: String::new(), farm_worker: false, raw_cmd: String::new(), raw_ext: String::new() }
    }
}

fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("ReelForge").join("config.json"))
}

impl Config {
    fn load() -> Config {
        config_path().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }
    fn save(&self) {
        if let Some(p) = config_path() {
            if let Some(d) = p.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            if let Ok(s) = serde_json::to_string_pretty(self) {
                let _ = std::fs::write(p, s);
            }
        }
    }
}

// ------------------------------------------------------------------------- background jobs

pub struct WaveMsg {
    pub media_id: u64,
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
}

pub enum ProxyMsg {
    Done(u64, PathBuf),
    Failed(u64, String),
}

/// Deferred edits requested from inside the properties panel (which cannot touch the project).
pub enum ExtAction {
    Ramp { id: u64, a: f64, b: f64, s0: f64, s1: f64, curve: RampCurve },
    Crossfade(u64),
    Slip(u64, f64),
    Slide(u64, f64),
    Roll(u64, u64, f64),
}

pub struct Ext {
    pub cfg: Config,
    bindings: Vec<(Cmd, Shortcut)>,
    pub rebinding: Option<Cmd>,
    pub shortcuts_open: bool,

    pub macros_open: bool,
    pub macro_rec: Option<Vec<Cmd>>,
    pub macro_name: String,

    pub frame: Option<(usize, usize, Vec<u8>)>,
    frame_stamp: u64,
    pub scopes_open: bool,
    scope_kind: usize,
    scope_tex: Option<TextureHandle>,
    scope_stamp: (u64, usize),

    pub source_open: bool,
    src_media: Option<u64>,
    src_t: f64,
    src_in: f64,
    src_out: f64,
    src_tex: Option<TextureHandle>,
    src_key: Option<(u64, i64)>,

    pub subs_open: bool,
    subs_stamp: Option<Instant>,
    pub markers_open: bool,

    pub loud_open: bool,
    loud_rx: Option<Receiver<Result<Loudness, String>>>,
    loud: Option<Loudness>,
    loud_msg: String,

    proxy_tx: Sender<ProxyMsg>,
    proxy_rx: Receiver<ProxyMsg>,
    proxy_busy: usize,
    wave_tx: Sender<WaveMsg>,
    wave_rx: Receiver<WaveMsg>,
    waves: HashMap<u64, TextureHandle>,
    wave_requested: HashSet<u64>,

    pub search: String,
    missing: HashSet<u64>,
    backup_rx: Option<Receiver<String>>,
    pub fx_name: String,
    pub bin_filter: String,
    pub preset_name: String,
    pub ramp_s0: f64,
    pub ramp_s1: f64,
    pub ramp_curve: RampCurve,
    pub action: Option<ExtAction>,
    pub rate: f64,

    pub mc_open: bool,
    pub mc_angles: Vec<u64>,
    pub mc_prog: Option<usize>,
    mc_rx: Option<Receiver<Result<Vec<f64>, String>>>,
    pub mc_msg: String,

    pub pl_open: bool,
    pub pl_pts: [(f32, f32); 4],
    pub pl_pick: Option<usize>,
    pl_rx: Option<Receiver<Result<PlanarData, String>>>,
    pl_data: Option<PlanarData>,
    pub pl_msg: String,
}

/// Result of tracking four corners: clip id, seconds into the clip tracking began, the four paths.
type PlanarData = (u64, f64, [Vec<crate::tracker::TrackPoint>; 4]);

impl Ext {
    pub fn load() -> Ext {
        let cfg = Config::load();
        let (proxy_tx, proxy_rx) = channel();
        let (wave_tx, wave_rx) = channel();
        let mut e = Ext {
            cfg,
            bindings: vec![],
            rebinding: None,
            shortcuts_open: false,
            macros_open: false,
            macro_rec: None,
            macro_name: "My macro".into(),
            frame: None,
            frame_stamp: 0,
            scopes_open: false,
            scope_kind: 0,
            scope_tex: None,
            scope_stamp: (0, 99),
            source_open: false,
            src_media: None,
            src_t: 0.0,
            src_in: 0.0,
            src_out: 0.0,
            src_tex: None,
            src_key: None,
            subs_open: false,
            subs_stamp: None,
            markers_open: false,
            loud_open: false,
            loud_rx: None,
            loud: None,
            loud_msg: String::new(),
            proxy_tx,
            proxy_rx,
            proxy_busy: 0,
            wave_tx,
            wave_rx,
            waves: HashMap::new(),
            wave_requested: HashSet::new(),
            search: String::new(),
            missing: HashSet::new(),
            backup_rx: None,
            fx_name: String::new(),
            bin_filter: String::new(),
            preset_name: String::new(),
            ramp_s0: 1.0,
            ramp_s1: 4.0,
            ramp_curve: RampCurve::Smooth,
            action: None,
            rate: 1.0,
            mc_open: false,
            mc_angles: vec![],
            mc_prog: None,
            mc_rx: None,
            mc_msg: String::new(),
            pl_open: false,
            pl_pts: [(0.3, 0.3), (0.7, 0.3), (0.7, 0.7), (0.3, 0.7)],
            pl_pick: None,
            pl_rx: None,
            pl_data: None,
            pl_msg: String::new(),
        };
        e.rebuild_bindings();
        e
    }

    pub fn rebuild_bindings(&mut self) {
        let mut v: Vec<(Cmd, Shortcut)> = Cmd::ALL
            .iter()
            .map(|c| {
                let sc = self.cfg.shortcuts.get(&format!("{c:?}")).and_then(|t| Shortcut::parse(t)).unwrap_or_else(|| c.default_shortcut());
                (*c, sc)
            })
            .collect();
        // most specific (most modifiers) first, so Ctrl+Shift+Z wins over Ctrl+Z
        v.sort_by(|a, b| b.1.weight().cmp(&a.1.weight()));
        self.bindings = v;
    }

    pub fn shortcut_of(&self, cmd: Cmd) -> Shortcut {
        self.bindings.iter().find(|(c, _)| *c == cmd).map(|(_, s)| *s).unwrap_or_else(|| cmd.default_shortcut())
    }

    pub fn is_missing(&self, id: u64) -> bool {
        self.missing.contains(&id)
    }

    pub fn save_cfg(&self) {
        self.cfg.save();
    }
}

fn ensure_order(p: &mut Project) {
    if let (Some(a), Some(b)) = (p.mark_in, p.mark_out) {
        if b < a {
            p.mark_in = Some(b);
            p.mark_out = Some(a);
        }
    }
}

// ------------------------------------------------------------------------- the editor

impl ReelForge {
    pub(super) fn ext_note_frame(&mut self, w: usize, h: usize, rgba: &[u8]) {
        if self.ext.scopes_open {
            self.ext.frame = Some((w, h, rgba.to_vec()));
            self.ext.frame_stamp += 1;
        }
    }

    /// Where an edit at the playhead should land: the selected clip's track, else track 0.
    fn edit_track(&self) -> usize {
        self.selected.and_then(|id| self.project.clip(id)).map(|c| c.track).unwrap_or(0)
    }

    fn marked_range(&self) -> Option<(f64, f64)> {
        match (self.project.mark_in, self.project.mark_out) {
            (Some(a), Some(b)) if (b - a).abs() > 0.02 => Some((a.min(b), a.max(b))),
            _ => self.selected.and_then(|id| self.project.clip(id)).map(|c| (c.start, c.end())),
        }
    }

    pub(super) fn exec(&mut self, ctx: &egui::Context, cmd: Cmd) {
        if let Some(rec) = self.ext.macro_rec.as_mut() {
            rec.push(cmd);
        }
        self.run_cmd(ctx, cmd);
    }

    fn run_cmd(&mut self, ctx: &egui::Context, cmd: Cmd) {
        let frame = 1.0 / self.project.fps.max(1) as f64;
        match cmd {
            Cmd::PlayPause => {
                self.playback_rate = 1.0;
                self.toggle_play(ctx);
            }
            Cmd::Split => self.split_at_playhead(),
            Cmd::Freeze => self.freeze_at_playhead(),
            Cmd::Delete => self.delete_selected(),
            Cmd::RippleDelete => {
                if let Some(id) = self.selected.take() {
                    self.push_undo();
                    self.project.ripple_delete(id);
                }
            }
            Cmd::Undo => self.do_undo(),
            Cmd::Redo => self.do_redo(),
            Cmd::Save => {
                self.save_project(false);
            }
            Cmd::Open => self.open_project(ctx),
            Cmd::Import => self.import_dialog(ctx),
            Cmd::Export => self.export_open = true,
            Cmd::StepBack => {
                self.stop_playback();
                self.playhead = (self.playhead - frame).max(0.0);
            }
            Cmd::StepFwd => {
                self.stop_playback();
                self.playhead += frame;
            }
            Cmd::GoStart => {
                self.stop_playback();
                self.playhead = 0.0;
            }
            Cmd::GoEnd => {
                self.stop_playback();
                self.playhead = self.project.duration();
            }
            Cmd::MarkIn => {
                self.push_undo();
                self.project.mark_in = Some(self.playhead);
                ensure_order(&mut self.project);
            }
            Cmd::MarkOut => {
                self.push_undo();
                self.project.mark_out = Some(self.playhead);
                ensure_order(&mut self.project);
            }
            Cmd::ClearInOut => {
                self.push_undo();
                self.project.mark_in = None;
                self.project.mark_out = None;
            }
            Cmd::AddMarker => {
                self.push_undo();
                let n = self.project.markers.len() + 1;
                let t = self.playhead;
                self.project.add_marker(t, format!("Marker {n}"));
            }
            Cmd::NextMarker => {
                if let Some(t) = self.project.next_marker(self.playhead) {
                    self.stop_playback();
                    self.playhead = t;
                }
            }
            Cmd::PrevMarker => {
                if let Some(t) = self.project.prev_marker(self.playhead) {
                    self.stop_playback();
                    self.playhead = t;
                }
            }
            Cmd::Lift | Cmd::Extract => match self.marked_range() {
                Some((a, b)) => {
                    self.push_undo();
                    self.project.cut_range(a, b, cmd == Cmd::Extract, None);
                    self.project.mark_in = None;
                    self.project.mark_out = None;
                    self.playhead = a;
                    self.status = format!("{} {} s.", if cmd == Cmd::Extract { "Extracted" } else { "Lifted" }, (b - a).round());
                }
                None => self.status = "Set in and out points (I / O) or select a clip first.".into(),
            },
            Cmd::ExtendEdit => {
                if let Some(id) = self.selected {
                    self.push_undo();
                    let t = self.playhead;
                    if !self.project.extend_edge(id, t) {
                        self.undo.pop();
                        self.status = "Nothing to extend: the clip is blocked or out of source media.".into();
                    }
                } else {
                    self.status = "Select a clip, put the playhead where its edge should go, then extend.".into();
                }
            }
            Cmd::InsertAtPlayhead | Cmd::OverwriteAtPlayhead => {
                let Some(mid) = self.selected_media else {
                    self.status = "Select a media item in the bin first.".into();
                    return;
                };
                self.push_undo();
                let (track, t) = (self.edit_track(), self.playhead);
                let id = if cmd == Cmd::InsertAtPlayhead { self.project.insert_media(mid, track, t) } else { self.project.overwrite_media(mid, track, t) };
                if let Some(id) = id {
                    self.selected = Some(id);
                }
            }
            Cmd::AddTitle => self.add_title_at_playhead(),
            Cmd::ShuttleSlower => {
                if self.playing {
                    let r = (self.playback_rate / 2.0).max(0.25);
                    self.shuttle(ctx, r);
                } else {
                    self.playhead = (self.playhead - 1.0).max(0.0);
                }
            }
            Cmd::ShuttlePause => self.stop_playback(),
            Cmd::ShuttleFaster => {
                let r = if self.playing { (self.playback_rate * 2.0).min(8.0) } else { 1.0 };
                self.shuttle(ctx, r);
            }
            Cmd::SourceMonitor => self.ext.source_open = !self.ext.source_open,
            Cmd::Scopes => self.ext.scopes_open = !self.ext.scopes_open,
        }
    }

    /// Restart playback from the playhead at a new speed.
    fn shuttle(&mut self, ctx: &egui::Context, rate: f64) {
        self.stop_playback();
        self.playback_rate = rate;
        self.toggle_play(ctx);
        self.status = format!("Playback {rate}x");
    }

    pub(super) fn ext_keys(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() || self.ext.rebinding.is_some() {
            return;
        }
        let list = self.ext.bindings.clone();
        for (cmd, sc) in list {
            let hit = ctx.input_mut(|i| i.consume_key(sc.mods(), sc.key));
            if hit {
                self.exec(ctx, cmd);
                break;
            }
        }
        // Ctrl+Y also redoes, and Backspace also deletes
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y)) {
            self.do_redo();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Backspace)) {
            self.delete_selected();
        }
    }

    // ------------------------------------------------------------- polling background work

    pub(super) fn ext_poll(&mut self, ctx: &egui::Context) {
        self.collab_poll(ctx);
        self.farm_poll(ctx);
        // deferred edits from the properties panel
        if let Some(a) = self.ext.action.take() {
            match a {
                ExtAction::Ramp { id, a, b, s0, s1, curve } => {
                    self.push_undo();
                    if self.project.speed_ramp_ex(id, a, b, s0, s1, 12, curve) {
                        self.status = "Speed ramp applied (12 steps).".into();
                    } else {
                        self.undo.pop();
                        self.status = "Pick a range at least 0.05 s long inside the clip.".into();
                    }
                }
                ExtAction::Slip(id, d) => {
                    self.push_undo();
                    if self.project.slip(id, d).abs() < 1e-9 {
                        self.undo.pop();
                        self.status = "Can't slip further: the clip is at the end of its source media.".into();
                    }
                }
                ExtAction::Slide(id, d) => {
                    self.push_undo();
                    if self.project.slide(id, d).abs() < 1e-9 {
                        self.undo.pop();
                        self.status = "Can't slide: the neighbouring clips have no more room.".into();
                    }
                }
                ExtAction::Roll(l, r, d) => {
                    self.push_undo();
                    if self.project.roll(l, r, d).abs() < 1e-9 {
                        self.undo.pop();
                        self.status = "Can't roll the edit any further.".into();
                    }
                }
                ExtAction::Crossfade(id) => {
                    self.push_undo();
                    if !self.project.audio_crossfade(id, 1.0) {
                        self.undo.pop();
                        self.status = "No clip touches the end of this one on its track.".into();
                    }
                }
            }
        }
        // loudness result
        if let Some(rx) = &self.ext.loud_rx {
            if let Ok(r) = rx.try_recv() {
                match r {
                    Ok(l) => {
                        self.ext.loud = Some(l);
                        self.ext.loud_msg.clear();
                    }
                    Err(e) => self.ext.loud_msg = e,
                }
                self.ext.loud_rx = None;
            }
        }
        // backup progress
        if let Some(rx) = &self.ext.backup_rx {
            let mut done = false;
            while let Ok(m) = rx.try_recv() {
                done = m.starts_with("Backup finished") || m.starts_with("Backup failed");
                self.status = m;
            }
            if done {
                self.ext.backup_rx = None;
            }
        }
        // proxies
        while let Ok(m) = self.ext.proxy_rx.try_recv() {
            self.ext.proxy_busy = self.ext.proxy_busy.saturating_sub(1);
            match m {
                ProxyMsg::Done(id, path) => {
                    if let Some(x) = self.project.media.iter_mut().find(|x| x.id == id) {
                        x.proxy = Some(path);
                    }
                    self.revision += 1;
                    self.status = "Proxy ready.".into();
                }
                ProxyMsg::Failed(id, e) => self.status = format!("Proxy for media {id} failed: {e}"),
            }
        }
        // audio waveforms
        let want: Vec<(u64, PathBuf)> = self.project.media.iter().filter(|m| m.kind == MediaKind::Audio && m.path.exists()).map(|m| (m.id, m.path.clone())).collect();
        for (id, path) in want {
            if self.ext.wave_requested.insert(id) {
                let (tx, ctx2) = (self.ext.wave_tx.clone(), ctx.clone());
                std::thread::spawn(move || {
                    let out = command("ffmpeg")
                        .args(["-v", "error", "-nostdin", "-i"])
                        .arg(&path)
                        .args([
                            "-filter_complex",
                            "aformat=channel_layouts=mono,showwavespic=s=1800x100:colors=0xA8F0C8:split_channels=0,format=rgba,colorkey=black:0.12:0.0",
                            "-frames:v",
                            "1",
                            "-f",
                            "image2pipe",
                            "-vcodec",
                            "png",
                            "-",
                        ])
                        .stdin(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .output();
                    if let Ok(o) = out {
                        if let Ok(img) = image::load_from_memory(&o.stdout) {
                            let img = img.to_rgba8();
                            let _ = tx.send(WaveMsg { media_id: id, w: img.width() as usize, h: img.height() as usize, rgba: img.into_raw() });
                            ctx2.request_repaint();
                        }
                    }
                });
            }
        }
        while let Ok(w) = self.ext.wave_rx.try_recv() {
            let img = egui::ColorImage::from_rgba_unmultiplied([w.w, w.h], &w.rgba);
            self.ext.waves.insert(w.media_id, ctx.load_texture(format!("wave{}", w.media_id), img, TextureOptions::LINEAR));
        }
    }

    /// Waveform texture for an audio media item, if ready.
    pub(super) fn ext_wave(&self, media_id: u64) -> Option<&TextureHandle> {
        self.ext.waves.get(&media_id)
    }

    // ------------------------------------------------------------- menus

    pub(super) fn ext_menus(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let item = |me: &Self, cmd: Cmd| format!("{}   {}", cmd.label(), me.ext.shortcut_of(cmd).text());
        ui.menu_button("Mark", |ui| {
            for cmd in [Cmd::MarkIn, Cmd::MarkOut, Cmd::ClearInOut, Cmd::AddMarker, Cmd::PrevMarker, Cmd::NextMarker] {
                if ui.button(item(self, cmd)).clicked() {
                    self.exec(ctx, cmd);
                    ui.close_menu();
                }
            }
            if ui.button("Markers list...").clicked() {
                self.ext.markers_open = true;
                ui.close_menu();
            }
            ui.separator();
            for cmd in [Cmd::Lift, Cmd::Extract, Cmd::RippleDelete, Cmd::ExtendEdit, Cmd::InsertAtPlayhead, Cmd::OverwriteAtPlayhead] {
                if ui.button(item(self, cmd)).clicked() {
                    self.exec(ctx, cmd);
                    ui.close_menu();
                }
            }
        });
        ui.menu_button("View", |ui| {
            if ui.checkbox(&mut self.ext.cfg.show_media, "Media panel").changed() | ui.checkbox(&mut self.ext.cfg.show_props, "Properties panel").changed() {
                self.ext.save_cfg();
            }
            ui.checkbox(&mut self.ext.scopes_open, "Video scopes");
            ui.checkbox(&mut self.ext.source_open, "Source monitor");
            ui.checkbox(&mut self.ext.loud_open, "Loudness meter");
            let mut px = self.project.use_proxies;
            if ui.checkbox(&mut px, "Use proxies for preview").changed() {
                self.project.use_proxies = px;
                self.revision += 1;
                self.preview_dirty = true;
            }
            ui.separator();
            ui.label(egui::RichText::new("Workspaces").weak());
            for (name, media, props, scopes, loud) in [("Editing", true, true, false, false), ("Color", false, true, true, false), ("Audio", true, true, false, true), ("Timeline focus", false, false, false, false)] {
                if ui.button(name).clicked() {
                    self.ext.cfg.show_media = media;
                    self.ext.cfg.show_props = props;
                    self.ext.scopes_open = scopes;
                    self.ext.loud_open = loud;
                    self.ext.save_cfg();
                    ui.close_menu();
                }
            }
        });
        ui.menu_button("Tools", |ui| {
            if ui.button("Planar track (corner pin)...").clicked() {
                self.ext.pl_open = true;
                ui.close_menu();
            }
            if ui.button("Version history...").clicked() {
                self.collab.versions_open = true;
                ui.close_menu();
            }
            if ui.button("Render farm...").clicked() {
                self.farm.open = true;
                ui.close_menu();
            }
            if ui.button("RAW converter...").clicked() {
                self.farm.raw_open = true;
                ui.close_menu();
            }
            if ui.button("Watch folders...").clicked() {
                self.collab.watch_open = true;
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Multicam...").clicked() {
                self.ext.mc_open = true;
                ui.close_menu();
            }
            if ui.button("Subtitles...").clicked() {
                self.ext.subs_open = true;
                ui.close_menu();
            }
            if ui.button("Measure loudness").clicked() {
                self.ext.loud_open = true;
                self.start_loudness(ctx);
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Generate proxies for all video").clicked() {
                self.make_proxies(ctx, None);
                ui.close_menu();
            }
            if ui.button("Relink selected media...").clicked() {
                self.relink_selected();
                ui.close_menu();
            }
            if ui.button("Check media files (missing / changed)").clicked() {
                self.check_media();
                ui.close_menu();
            }
            if ui.button("Back up and verify media...").clicked() {
                self.backup_media(ctx);
                ui.close_menu();
            }
            if ui.button("Export ingest log (CSV)...").clicked() {
                self.export_ingest();
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Export EDL (CMX3600)...").clicked() {
                self.export_edl();
                ui.close_menu();
            }
            if ui.button("Export Final Cut Pro 7 XML (Premiere / Resolve)...").clicked() {
                self.export_xml(false);
                ui.close_menu();
            }
            if ui.button("Export OpenTimelineIO (.otio)...").clicked() {
                self.export_xml(true);
                ui.close_menu();
            }
            if ui.button("Import Final Cut Pro 7 XML...").clicked() {
                self.import_xml();
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Import subtitles (.srt)...").clicked() {
                self.import_srt();
                ui.close_menu();
            }
            if ui.button("Export subtitles (.srt)...").clicked() {
                self.export_srt();
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Keyboard shortcuts...").clicked() {
                self.ext.shortcuts_open = true;
                ui.close_menu();
            }
            if ui.button("Macros...").clicked() {
                self.ext.macros_open = true;
                ui.close_menu();
            }
            let macros = self.ext.cfg.macros.clone();
            if !macros.is_empty() {
                ui.separator();
                ui.label(egui::RichText::new("Run macro").weak());
                for m in macros {
                    if ui.button(&m.name).clicked() {
                        self.run_macro(ctx, &m);
                        ui.close_menu();
                    }
                }
            }
        });
    }

    fn run_macro(&mut self, ctx: &egui::Context, m: &Macro) {
        for c in &m.steps {
            self.run_cmd(ctx, *c);
        }
        self.status = format!("Ran macro \"{}\" ({} steps).", m.name, m.steps.len());
    }

    // ------------------------------------------------------------- files

    fn export_edl(&mut self) {
        let name = self.project_path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Untitled".into());
        if let Some(path) = rfd::FileDialog::new().set_file_name(format!("{name}.edl")).add_filter("EDL", &["edl"]).save_file() {
            let text = interchange::to_edl(&self.project, &name);
            self.status = match std::fs::write(&path, text) {
                Ok(_) => format!("Wrote {}", path.display()),
                Err(e) => format!("Could not write EDL: {e}"),
            };
        }
    }

    fn export_xml(&mut self, otio: bool) {
        let name = self.project_path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Untitled".into());
        let (ext, label) = if otio { ("otio", "OpenTimelineIO") } else { ("xml", "Final Cut Pro XML") };
        if let Some(path) = rfd::FileDialog::new().set_file_name(format!("{name}.{ext}")).add_filter(label, &[ext]).save_file() {
            let text = if otio { interchange::to_otio(&self.project, &name) } else { interchange::to_xmeml(&self.project, &name) };
            self.status = match std::fs::write(&path, text) {
                Ok(_) => format!("Wrote {}", path.display()),
                Err(e) => format!("Could not write file: {e}"),
            };
        }
    }

    /// Rebuild a timeline from an FCP7 XML file using media already imported into this project
    /// (matched by file path, then by file name). Unmatched files are listed so they can be imported first.
    fn import_xml(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("Final Cut Pro XML", &["xml"]).pick_file() else { return };
        let text = match std::fs::read(&path) {
            Ok(b) => String::from_utf8_lossy(&b).into_owned(),
            Err(e) => {
                self.status = format!("Could not read file: {e}");
                return;
            }
        };
        let Some((_fps, clips)) = interchange::parse_xmeml(&text) else {
            self.status = "That does not look like a Final Cut Pro 7 XML sequence.".into();
            return;
        };
        let norm = |s: &str| s.replace('\\', "/").to_lowercase();
        let base = |s: &str| norm(s).rsplit('/').next().unwrap_or("").to_string();
        let mut missing: Vec<String> = vec![];
        let mut placed = 0;
        self.push_undo();
        let first_new_track = self.project.tracks.len();
        let max_track = clips.iter().map(|c| c.track).max().unwrap_or(0);
        for _ in 0..=max_track {
            self.project.add_track();
        }
        for c in &clips {
            let Some(m) = self.project.media.iter().find(|m| norm(&m.path.to_string_lossy()) == norm(&c.path)).or_else(|| self.project.media.iter().find(|m| base(&m.path.to_string_lossy()) == base(&c.path))).cloned() else {
                if !missing.contains(&c.name) {
                    missing.push(c.name.clone());
                }
                continue;
            };
            // linked audio of a video clip is already carried by the video clip
            if c.audio && m.kind != MediaKind::Audio {
                continue;
            }
            let tr = first_new_track + c.track;
            let span = (c.out_pt - c.in_pt).max(0.05);
            if let Some(id) = self.project.add_subclip(m.id, tr, c.start, c.in_pt, c.in_pt + span, true) {
                if let Some(cl) = self.project.clip_mut(id) {
                    if m.kind == MediaKind::Video && (c.speed.abs() - 1.0).abs() > 1e-6 && c.speed != 0.0 {
                        cl.speed = c.speed.abs();
                        cl.reverse = c.speed < 0.0;
                        cl.duration = span / cl.speed;
                    } else if c.speed < 0.0 {
                        cl.reverse = true;
                    }
                }
                placed += 1;
            }
        }
        self.revision += 1;
        self.preview_dirty = true;
        self.status = if missing.is_empty() {
            format!("Imported {placed} clips from {}", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
        } else {
            format!("Imported {placed} clips. Not in this project yet (import these files and run it again): {}", missing.join(", "))
        };
    }

    fn import_srt(&mut self) {
        if let Some(path) = rfd::FileDialog::new().add_filter("SubRip", &["srt"]).pick_file() {
            match std::fs::read(&path) {
                Ok(bytes) => {
                    let subs = interchange::parse_srt(&String::from_utf8_lossy(&bytes));
                    if subs.is_empty() {
                        self.status = "No subtitles found in that file.".into();
                    } else {
                        self.push_undo();
                        self.status = format!("Imported {} subtitles.", subs.len());
                        self.project.subtitles = subs;
                        self.ext.subs_open = true;
                    }
                }
                Err(e) => self.status = format!("Could not read file: {e}"),
            }
        }
    }

    fn export_srt(&mut self) {
        if self.project.subtitles.is_empty() {
            self.status = "There are no subtitles to export.".into();
            return;
        }
        if let Some(path) = rfd::FileDialog::new().set_file_name("subtitles.srt").add_filter("SubRip", &["srt"]).save_file() {
            self.status = match std::fs::write(&path, interchange::to_srt(&self.project.subtitles)) {
                Ok(_) => format!("Wrote {}", path.display()),
                Err(e) => format!("Could not write file: {e}"),
            };
        }
    }

    /// Flag media whose file is gone or whose size no longer matches what was imported.
    fn check_media(&mut self) {
        self.ext.missing.clear();
        let (mut gone, mut changed) = (0, 0);
        for m in &self.project.media {
            match std::fs::metadata(&m.path) {
                Err(_) => {
                    self.ext.missing.insert(m.id);
                    gone += 1;
                }
                Ok(md) if m.info.size_bytes > 0 && md.len() != m.info.size_bytes => changed += 1,
                Ok(_) => {}
            }
        }
        self.status = if gone == 0 && changed == 0 {
            "All media files are present and unchanged.".into()
        } else {
            format!("{gone} media file(s) missing (select one and use Tools > Relink), {changed} changed since import.")
        };
    }

    fn export_ingest(&mut self) {
        if self.project.ingest.is_empty() {
            self.status = "Nothing has been imported yet.".into();
            return;
        }
        if let Some(path) = rfd::FileDialog::new().set_file_name("ingest_log.csv").add_filter("CSV", &["csv"]).save_file() {
            self.status = match std::fs::write(&path, interchange::to_ingest_csv(&self.project)) {
                Ok(_) => format!("Wrote {}", path.display()),
                Err(e) => format!("Could not write the log: {e}"),
            };
        }
    }

    /// Copy every media file into a folder and verify each copy byte for byte; writes a log.
    fn backup_media(&mut self, ctx: &egui::Context) {
        if self.ext.backup_rx.is_some() {
            return;
        }
        let Some(dir) = rfd::FileDialog::new().set_title("Choose a folder for the backup").pick_folder() else { return };
        let files: Vec<(u64, PathBuf)> = self.project.media.iter().map(|m| (m.id, m.path.clone())).collect();
        if files.is_empty() {
            self.status = "There is no media to back up.".into();
            return;
        }
        let (tx, rx) = channel();
        let ctx2 = ctx.clone();
        self.status = format!("Backing up {} file(s)...", files.len());
        self.ext.backup_rx = Some(rx);
        std::thread::spawn(move || {
            let send = |m: String| {
                let _ = tx.send(m);
                ctx2.request_repaint();
            };
            let mut log = String::from("file,status,bytes\n");
            let (mut ok, mut bad) = (0usize, 0usize);
            let total = files.len();
            for (n, (id, src)) in files.into_iter().enumerate() {
                let name = src.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| format!("media{id}"));
                let dst = dir.join(format!("{id}_{name}"));
                send(format!("Backing up {}/{}: {name}", n + 1, total));
                let status = match std::fs::copy(&src, &dst) {
                    Err(e) => format!("COPY FAILED ({e})"),
                    Ok(_) => match same_bytes(&src, &dst) {
                        Ok(true) => "OK".to_string(),
                        Ok(false) => "MISMATCH".to_string(),
                        Err(e) => format!("VERIFY FAILED ({e})"),
                    },
                };
                if status == "OK" {
                    ok += 1;
                } else {
                    bad += 1;
                }
                let bytes = std::fs::metadata(&dst).map(|m| m.len()).unwrap_or(0);
                log.push_str(&format!("\"{}\",{},{}\n", name.replace('"', "'"), status, bytes));
            }
            let _ = std::fs::write(dir.join("backup_log.csv"), log);
            send(if bad == 0 { format!("Backup finished: {ok} file(s) copied and verified.") } else { format!("Backup finished with problems: {ok} verified, {bad} failed (see backup_log.csv).") });
        });
    }

    fn relink_selected(&mut self) {
        let Some(id) = self.selected_media else {
            self.status = "Select the media item to relink in the bin first.".into();
            return;
        };
        if let Some(path) = rfd::FileDialog::new().set_title("Locate the file").pick_file() {
            self.push_undo();
            if let Some(m) = self.project.media.iter_mut().find(|m| m.id == id) {
                m.path = path;
                m.proxy = None;
            }
            self.status = "Media relinked.".into();
        }
    }

    fn make_proxies(&mut self, ctx: &egui::Context, only: Option<u64>) {
        let dir = std::env::temp_dir().join("ReelForgeProxies");
        let _ = std::fs::create_dir_all(&dir);
        let jobs: Vec<(u64, PathBuf, PathBuf, bool)> = self
            .project
            .media
            .iter()
            .filter(|m| m.kind == MediaKind::Video && only.map_or(true, |o| o == m.id))
            .filter(|m| m.proxy.as_ref().map_or(true, |p| !p.exists()))
            .map(|m| (m.id, m.path.clone(), dir.join(format!("proxy_{}.mp4", m.id)), m.has_audio))
            .collect();
        if jobs.is_empty() {
            self.status = "Every video already has a proxy.".into();
            return;
        }
        self.ext.proxy_busy += jobs.len();
        self.status = format!("Making {} proxy file(s) in the background...", jobs.len());
        let (tx, ctx2) = (self.ext.proxy_tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            for (id, src, dst, has_audio) in jobs {
                let mut cmd = command("ffmpeg");
                cmd.args(["-y", "-v", "error", "-nostdin", "-i"]).arg(&src).args(["-vf", "scale=-2:360", "-c:v", "libx264", "-preset", "veryfast", "-crf", "28", "-pix_fmt", "yuv420p"]);
                if has_audio {
                    cmd.args(["-c:a", "aac", "-b:a", "96k"]);
                } else {
                    cmd.arg("-an");
                }
                let r = cmd.arg(&dst).stdin(std::process::Stdio::null()).output();
                let _ = tx.send(match r {
                    Ok(o) if o.status.success() => ProxyMsg::Done(id, dst),
                    Ok(o) => ProxyMsg::Failed(id, String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("ffmpeg error").to_string()),
                    Err(e) => ProxyMsg::Failed(id, e.to_string()),
                });
                ctx2.request_repaint();
            }
        });
    }

    fn start_loudness(&mut self, ctx: &egui::Context) {
        if self.ext.loud_rx.is_some() {
            return;
        }
        let p = match (self.project.mark_in, self.project.mark_out) {
            (Some(a), Some(b)) if b - a > 0.1 => self.project.range(a, b),
            _ => self.project.clone(),
        };
        let (tx, rx) = channel();
        let ctx2 = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(render::measure_loudness(&p));
            ctx2.request_repaint();
        });
        self.ext.loud_msg = "Measuring...".into();
        self.ext.loud_rx = Some(rx);
    }

    /// Start an undo step for an edit made in a floating window; rapid edits share one step.
    fn ext_begin_edit(&mut self) {
        let recent = matches!(self.ext.subs_stamp, Some(t) if t.elapsed().as_millis() < 700);
        if recent {
            self.revision += 1;
            self.preview_dirty = true;
        } else {
            self.push_undo();
        }
        self.ext.subs_stamp = Some(Instant::now());
    }

    // ------------------------------------------------------------- floating windows

    pub(super) fn ext_windows(&mut self, ctx: &egui::Context) {
        self.win_shortcuts(ctx);
        self.win_macros(ctx);
        self.win_scopes(ctx);
        self.win_source(ctx);
        self.win_subs(ctx);
        self.win_markers(ctx);
        self.win_loudness(ctx);
        self.win_multicam(ctx);
        self.win_planar(ctx);
        self.win_versions(ctx);
        self.win_watch(ctx);
        self.win_farm(ctx);
        self.win_raw(ctx);
    }

    fn win_shortcuts(&mut self, ctx: &egui::Context) {
        if !self.ext.shortcuts_open {
            self.ext.rebinding = None;
            return;
        }
        // capture the next key press while rebinding
        if let Some(cmd) = self.ext.rebinding {
            let ev = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Key { key, pressed: true, modifiers, .. } => Some((*key, *modifiers)),
                    _ => None,
                })
            });
            if let Some((key, m)) = ev {
                if key == egui::Key::Escape {
                    self.ext.rebinding = None;
                } else {
                    let sc = Shortcut { key, ctrl: m.command || m.ctrl, shift: m.shift, alt: m.alt };
                    self.ext.cfg.shortcuts.insert(format!("{cmd:?}"), sc.text());
                    self.ext.rebuild_bindings();
                    self.ext.save_cfg();
                    self.ext.rebinding = None;
                }
            }
        }
        let mut open = true;
        let mut reset = false;
        egui::Window::new("Keyboard shortcuts").open(&mut open).default_width(420.0).show(ctx, |ui| {
            ui.label(egui::RichText::new("Click Change, then press the new key combination (Esc cancels).").weak());
            egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                egui::Grid::new("shortcut_grid").num_columns(3).striped(true).show(ui, |ui| {
                    for cmd in Cmd::ALL {
                        ui.label(cmd.label());
                        ui.monospace(self.ext.shortcut_of(cmd).text());
                        let waiting = self.ext.rebinding == Some(cmd);
                        if ui.button(if waiting { "Press keys..." } else { "Change" }).clicked() {
                            self.ext.rebinding = Some(cmd);
                        }
                        ui.end_row();
                    }
                });
            });
            if ui.button("Reset all to defaults").clicked() {
                reset = true;
            }
        });
        if reset {
            self.ext.cfg.shortcuts.clear();
            self.ext.rebuild_bindings();
            self.ext.save_cfg();
        }
        self.ext.shortcuts_open = open;
    }

    fn win_macros(&mut self, ctx: &egui::Context) {
        if !self.ext.macros_open {
            return;
        }
        let mut open = true;
        let mut run: Option<Macro> = None;
        let mut delete: Option<usize> = None;
        let mut save = false;
        let mut start = false;
        let recording = self.ext.macro_rec.as_ref().map(|r| r.len());
        egui::Window::new("Macros").open(&mut open).default_width(360.0).show(ctx, |ui| {
            ui.label(egui::RichText::new("A macro replays a list of editing commands. Record one by performing the commands (keys or menus) while recording.").weak());
            ui.separator();
            match recording {
                Some(n) => {
                    ui.label(format!("Recording... {n} step(s)"));
                    ui.horizontal(|ui| {
                        ui.label("Name");
                        ui.text_edit_singleline(&mut self.ext.macro_name);
                    });
                    if ui.button("Stop and save").clicked() {
                        save = true;
                    }
                }
                None => {
                    if ui.button("Start recording").clicked() {
                        start = true;
                    }
                }
            }
            ui.separator();
            for (i, m) in self.ext.cfg.macros.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("{} ({} steps)", m.name, m.steps.len()));
                    if ui.small_button("Run").clicked() {
                        run = Some(m.clone());
                    }
                    if ui.small_button("Delete").clicked() {
                        delete = Some(i);
                    }
                });
            }
            if self.ext.cfg.macros.is_empty() {
                ui.label(egui::RichText::new("No macros yet.").weak());
            }
        });
        if start {
            self.ext.macro_rec = Some(vec![]);
        }
        if save {
            if let Some(steps) = self.ext.macro_rec.take() {
                if !steps.is_empty() {
                    let name = self.ext.macro_name.trim().to_string();
                    self.ext.cfg.macros.push(Macro { name: if name.is_empty() { "Macro".into() } else { name }, steps });
                    self.ext.save_cfg();
                }
            }
        }
        if let Some(i) = delete {
            self.ext.cfg.macros.remove(i);
            self.ext.save_cfg();
        }
        if let Some(m) = run {
            self.run_macro(ctx, &m);
        }
        self.ext.macros_open = open;
    }

    fn win_scopes(&mut self, ctx: &egui::Context) {
        if !self.ext.scopes_open {
            return;
        }
        let mut open = true;
        if self.ext.frame.is_none() {
            self.preview_dirty = true;
        }
        let stamp = (self.ext.frame_stamp, self.ext.scope_kind);
        if stamp != self.ext.scope_stamp {
            if let Some((fw, fh, rgba)) = &self.ext.frame {
                let img = match self.ext.scope_kind {
                    0 => scopes::waveform(rgba, *fw, *fh, 512, 256),
                    1 => scopes::parade(rgba, *fw, *fh, 512, 256),
                    2 => scopes::vectorscope(rgba, *fw, *fh, 256),
                    _ => scopes::histogram(rgba, *fw, *fh, 512, 256),
                };
                let ci = egui::ColorImage::from_rgba_unmultiplied([img.w, img.h], &img.rgba);
                match &mut self.ext.scope_tex {
                    Some(t) => t.set(ci, TextureOptions::LINEAR),
                    None => self.ext.scope_tex = Some(ctx.load_texture("scope", ci, TextureOptions::LINEAR)),
                }
                self.ext.scope_stamp = stamp;
            }
        }
        egui::Window::new("Video scopes").open(&mut open).default_width(540.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                for (i, n) in ["Waveform", "RGB parade", "Vectorscope", "Histogram"].iter().enumerate() {
                    ui.selectable_value(&mut self.ext.scope_kind, i, *n);
                }
            });
            match &self.ext.scope_tex {
                Some(t) => {
                    let w = ui.available_width().min(520.0);
                    let sz = t.size_vec2();
                    ui.image((t.id(), Vec2::new(w, w * sz.y / sz.x)));
                }
                None => {
                    ui.label(egui::RichText::new("Waiting for a preview frame...").weak());
                }
            }
            ui.label(egui::RichText::new("Shows the frame at the playhead.").weak().small());
        });
        self.ext.scopes_open = open;
    }

    fn win_source(&mut self, ctx: &egui::Context) {
        if !self.ext.source_open {
            return;
        }
        let mut open = true;
        if self.ext.src_media.is_none() {
            self.ext.src_media = self.selected_media.or_else(|| self.project.media.first().map(|m| m.id));
        }
        let media: Vec<MediaItem> = self.project.media.clone();
        let mut insert: Option<bool> = None;
        egui::Window::new("Source monitor").open(&mut open).default_width(420.0).show(ctx, |ui| {
            let cur = self.ext.src_media.and_then(|id| media.iter().find(|m| m.id == id)).cloned();
            egui::ComboBox::from_id_salt("src_media").selected_text(cur.as_ref().map(|m| elide(&m.name, 34)).unwrap_or_else(|| "(no media)".into())).show_ui(ui, |ui| {
                for m in &media {
                    if ui.selectable_label(Some(m.id) == self.ext.src_media, &m.name).clicked() {
                        self.ext.src_media = Some(m.id);
                        self.ext.src_t = 0.0;
                        self.ext.src_in = 0.0;
                        self.ext.src_out = m.duration;
                        self.ext.src_key = None;
                    }
                }
            });
            let Some(m) = cur else { return };
            if self.ext.src_out <= 0.0 || self.ext.src_out > m.duration {
                self.ext.src_out = m.duration;
            }
            let dur = m.duration.max(0.1);
            let resp = ui.add(egui::Slider::new(&mut self.ext.src_t, 0.0..=dur).text("Time").custom_formatter(|v, _| fmt_time(v)));
            if m.kind != MediaKind::Audio {
                let key = (m.id, (self.ext.src_t * 10.0).round() as i64);
                if self.ext.src_key != Some(key) && !resp.dragged() {
                    self.ext.src_key = Some(key);
                    let t = if m.kind == MediaKind::Image { None } else { Some(self.ext.src_t) };
                    let path = if self.project.use_proxies { m.proxy.clone().filter(|p| p.exists()).unwrap_or(m.path.clone()) } else { m.path.clone() };
                    if let Some(img) = preview::grab_frame(&path, t, 480, 270) {
                        let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
                        match &mut self.ext.src_tex {
                            Some(t) => t.set(ci, TextureOptions::LINEAR),
                            None => self.ext.src_tex = Some(ctx.load_texture("source", ci, TextureOptions::LINEAR)),
                        }
                    }
                }
                if let Some(t) = &self.ext.src_tex {
                    let sz = t.size_vec2();
                    let w = ui.available_width().min(440.0);
                    ui.image((t.id(), Vec2::new(w, w * sz.y / sz.x)));
                }
            }
            ui.horizontal(|ui| {
                if ui.button("Set in").clicked() {
                    self.ext.src_in = self.ext.src_t.min(self.ext.src_out - 0.05).max(0.0);
                }
                if ui.button("Set out").clicked() {
                    self.ext.src_out = self.ext.src_t.max(self.ext.src_in + 0.05).min(dur);
                }
                ui.label(format!("In {}   Out {}   Length {}", fmt_time(self.ext.src_in), fmt_time(self.ext.src_out), fmt_time((self.ext.src_out - self.ext.src_in).max(0.0))));
            });
            ui.horizontal(|ui| {
                if ui.button("Insert at playhead").clicked() {
                    insert = Some(false);
                }
                if ui.button("Overwrite at playhead").clicked() {
                    insert = Some(true);
                }
            });
            ui.label(egui::RichText::new("Mark a part of the source, then add only that part to the selected clip's track.").weak().small());
        });
        if let (Some(overwrite), Some(mid)) = (insert, self.ext.src_media) {
            self.push_undo();
            let (track, t, a, b) = (self.edit_track(), self.playhead, self.ext.src_in, self.ext.src_out);
            match self.project.add_subclip(mid, track, t, a, b, overwrite) {
                Some(id) => self.selected = Some(id),
                None => {
                    self.undo.pop();
                }
            }
        }
        self.ext.source_open = open;
    }

    fn win_subs(&mut self, ctx: &egui::Context) {
        if !self.ext.subs_open {
            return;
        }
        let mut open = true;
        let mut subs = self.project.subtitles.clone();
        let mut remove: Option<usize> = None;
        let mut add = false;
        egui::Window::new("Subtitles").open(&mut open).default_width(460.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("+ Add at playhead").clicked() {
                    add = true;
                }
                ui.label(egui::RichText::new("Burned into exports (see Export > Subtitles). Import / export .srt from the Tools menu.").weak().small());
            });
            egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                for (i, sb) in subs.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut sb.start).speed(0.05).fixed_decimals(2).suffix(" s"));
                        ui.label("to");
                        ui.add(egui::DragValue::new(&mut sb.end).speed(0.05).fixed_decimals(2).suffix(" s"));
                        if ui.small_button("Delete").clicked() {
                            remove = Some(i);
                        }
                    });
                    ui.add(egui::TextEdit::multiline(&mut sb.text).desired_rows(1).desired_width(f32::INFINITY));
                    ui.separator();
                }
            });
        });
        if add {
            let t = self.playhead;
            subs.push(Subtitle { start: t, end: t + 2.0, text: "New subtitle".into() });
        }
        if let Some(i) = remove {
            subs.remove(i);
        }
        for sb in subs.iter_mut() {
            if sb.end < sb.start + 0.05 {
                sb.end = sb.start + 0.05;
            }
        }
        if add || remove.is_some() || subs != self.project.subtitles {
            subs.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
            self.ext_begin_edit();
            self.project.subtitles = subs;
        }
        self.ext.subs_open = open;
    }

    fn win_markers(&mut self, ctx: &egui::Context) {
        if !self.ext.markers_open {
            return;
        }
        let mut open = true;
        let mut marks = self.project.markers.clone();
        let mut remove: Option<usize> = None;
        let mut goto: Option<f64> = None;
        egui::Window::new("Markers").open(&mut open).default_width(360.0).show(ctx, |ui| {
            if marks.is_empty() {
                ui.label(egui::RichText::new("No markers. Press M to add one at the playhead.").weak());
            }
            for (i, m) in marks.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut m.t).speed(0.05).fixed_decimals(2).suffix(" s"));
                    ui.text_edit_singleline(&mut m.name);
                    if ui.small_button("Go").clicked() {
                        goto = Some(m.t);
                    }
                    if ui.small_button("Delete").clicked() {
                        remove = Some(i);
                    }
                });
            }
        });
        if let Some(i) = remove {
            marks.remove(i);
        }
        if marks != self.project.markers {
            marks.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal));
            self.ext_begin_edit();
            self.project.markers = marks;
        }
        if let Some(t) = goto {
            self.stop_playback();
            self.playhead = t;
        }
        self.ext.markers_open = open;
    }

    /// Run audio sync for the angle clips on a worker thread; result = lag of each vs the first.
    fn start_multicam_sync(&mut self, ctx: &egui::Context) {
        let mut paths = vec![];
        for id in &self.ext.mc_angles {
            let p = self.project.clip(*id).and_then(|c| self.project.media(c.media_id)).map(|m| m.path.clone());
            match p {
                Some(p) => paths.push(p),
                None => {
                    self.ext.mc_msg = "An angle no longer exists.".into();
                    return;
                }
            }
        }
        let (tx, rx) = channel();
        let ctx2 = ctx.clone();
        std::thread::spawn(move || {
            let mut lags = vec![0.0];
            let mut res = Ok(());
            for p in paths.iter().skip(1) {
                match crate::sync::media_offset(&paths[0], p) {
                    Ok((lag, conf)) if conf >= 6.0 => lags.push(lag),
                    Ok((_, conf)) => {
                        res = Err(format!("Could not find a confident match for {} (score {conf:.1}). Do the cameras record the same sound?", p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()));
                        break;
                    }
                    Err(e) => {
                        res = Err(e);
                        break;
                    }
                }
            }
            let _ = tx.send(res.map(|_| lags));
            ctx2.request_repaint();
        });
        self.ext.mc_rx = Some(rx);
        self.ext.mc_msg = "Analysing audio...".into();
    }

    fn win_multicam(&mut self, ctx: &egui::Context) {
        if !self.ext.mc_open {
            return;
        }
        if let Some(rx) = &self.ext.mc_rx {
            match rx.try_recv() {
                Ok(res) => {
                    self.ext.mc_rx = None;
                    match res {
                        Ok(lags) => {
                            let angles = self.ext.mc_angles.clone();
                            self.push_undo();
                            if self.project.multicam_align(&angles, &lags) {
                                self.ext.mc_msg = format!("Synced {} angles by sound.", angles.len());
                            }
                            self.revision += 1;
                            self.preview_dirty = true;
                        }
                        Err(e) => self.ext.mc_msg = e,
                    }
                }
                Err(_) => ctx.request_repaint_after(std::time::Duration::from_millis(200)),
            }
        }
        let mut open = true;
        let (mut add_sel, mut clear, mut sync) = (false, false, false);
        let mut cut: Option<u64> = None;
        let busy = self.ext.mc_rx.is_some();
        let angles = self.ext.mc_angles.clone();
        let names: Vec<(u64, String)> = angles
            .iter()
            .map(|id| {
                let n = self.project.clip(*id).and_then(|c| self.project.media(c.media_id)).map(|m| m.name.clone()).unwrap_or_else(|| "(missing)".into());
                (*id, n)
            })
            .collect();
        let has_sel = self.selected.is_some();
        let msg = self.ext.mc_msg.clone();
        egui::Window::new("Multicam").open(&mut open).default_width(340.0).show(ctx, |ui| {
            ui.label(egui::RichText::new("Put each camera on its own track, select a clip, press Add. Sync lines the angles up by their sound; then switch angles at the playhead.").weak().small());
            ui.horizontal(|ui| {
                add_sel = ui.add_enabled(has_sel, egui::Button::new("Add selected clip as angle")).clicked();
                clear = ui.button("Clear").clicked();
            });
            for (i, (id, n)) in names.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.monospace(format!("{}", i + 1));
                    ui.label(n);
                    if ui.small_button("Cut to this angle").clicked() {
                        cut = Some(*id);
                    }
                });
            }
            ui.separator();
            ui.horizontal(|ui| {
                sync = ui.add_enabled(!busy && angles.len() >= 2, egui::Button::new("Sync by audio")).clicked();
                if busy {
                    ui.spinner();
                }
            });
            if !msg.is_empty() {
                ui.label(&msg);
            }
        });
        if clear {
            self.ext.mc_angles.clear();
            self.ext.mc_msg.clear();
        }
        if add_sel {
            if let Some(id) = self.selected {
                if !self.ext.mc_angles.contains(&id) && self.ext.mc_angles.len() < 9 {
                    self.ext.mc_angles.push(id);
                }
            }
        }
        if sync {
            self.start_multicam_sync(ctx);
        }
        if let Some(id) = cut {
            let t = self.playhead;
            let prog = match self.ext.mc_prog.filter(|p| *p < self.project.tracks.len()) {
                Some(p) => p,
                None => {
                    self.project.add_track();
                    let p = self.project.tracks.len() - 1;
                    self.project.tracks[p].name = "Program".into();
                    self.ext.mc_prog = Some(p);
                    p
                }
            };
            self.push_undo();
            if self.project.multicam_cut(id, prog, t).is_some() {
                for aid in self.ext.mc_angles.clone() {
                    if let Some(tr) = self.project.clip(aid).map(|c| c.track) {
                        if tr != prog {
                            self.project.tracks[tr].muted = true;
                        }
                    }
                }
                self.ext.mc_msg = format!("Cut at {t:.2} s.");
            } else {
                self.ext.mc_msg = "Playhead is outside that angle, or the clip is retimed.".into();
            }
            self.revision += 1;
            self.preview_dirty = true;
        }
        self.ext.mc_open = open;
    }

    fn win_planar(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.ext.pl_rx {
            if let Ok(r) = rx.try_recv() {
                self.ext.pl_rx = None;
                match r {
                    Ok(d) => {
                        self.ext.pl_msg = format!("Tracked {} samples per corner. Select the clip to pin and press Attach.", d.2[0].len());
                        self.ext.pl_data = Some(d);
                    }
                    Err(e) => self.ext.pl_msg = e,
                }
            }
        }
        if !self.ext.pl_open {
            self.ext.pl_pick = None;
            return;
        }
        let mut open = true;
        let (mut track, mut attach) = (false, false);
        let src_sel = self.selected.and_then(|id| self.project.clip(id)).cloned();
        egui::Window::new("Planar track (corner pin)").open(&mut open).default_width(380.0).show(ctx, |ui| {
            ui.label(egui::RichText::new("Select the footage clip, mark the four corners of the surface (a screen, a sign) on the preview, and track. Then select the clip that should be pinned there and press Attach.").weak().small());
            ui.separator();
            for (i, name) in ["Top-left", "Top-right", "Bottom-right", "Bottom-left"].iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(*name);
                    ui.add(egui::DragValue::new(&mut self.ext.pl_pts[i].0).speed(0.002).range(0.0..=1.0).fixed_decimals(3).prefix("x "));
                    ui.add(egui::DragValue::new(&mut self.ext.pl_pts[i].1).speed(0.002).range(0.0..=1.0).fixed_decimals(3).prefix("y "));
                    if ui.selectable_label(self.ext.pl_pick == Some(i), "Pick on preview").clicked() {
                        self.ext.pl_pick = if self.ext.pl_pick == Some(i) { None } else { Some(i) };
                    }
                });
            }
            ui.separator();
            let busy = self.ext.pl_rx.is_some();
            ui.horizontal(|ui| {
                let ok = src_sel.as_ref().map_or(false, |c| !c.generated() && !c.reverse && !c.freeze);
                if ui.add_enabled(ok && !busy, egui::Button::new("Track from playhead")).clicked() {
                    track = true;
                }
                if busy {
                    ui.spinner();
                }
                if ui.add_enabled(self.ext.pl_data.is_some() && src_sel.is_some(), egui::Button::new("Attach to selected clip")).clicked() {
                    attach = true;
                }
            });
            if !self.ext.pl_msg.is_empty() {
                ui.label(&self.ext.pl_msg);
            }
            ui.label(egui::RichText::new("Each corner is followed on its own (template matching), so this works best on flat, well-lit, textured surfaces and may drift if a corner is hidden or blurred.").weak().small());
        });
        if track {
            if let Some(c) = src_sel {
                if let Some(m) = self.project.media(c.media_id).cloned() {
                    let t0 = (self.playhead - c.start).clamp(0.0, (c.duration - 0.2).max(0.0));
                    let from = c.in_point + t0 * c.speed;
                    let secs = (c.duration - t0) * c.speed;
                    let (tx, rx) = channel();
                    let (path, pts, id, ctx2) = (m.path.clone(), self.ext.pl_pts, c.id, ctx.clone());
                    std::thread::spawn(move || {
                        let r = crate::tracker::track_corners(&path, from, secs, m.width, m.height, pts).map(|p| (id, t0, p));
                        let _ = tx.send(r);
                        ctx2.request_repaint();
                    });
                    self.ext.pl_rx = Some(rx);
                    self.ext.pl_data = None;
                    self.ext.pl_msg = "Tracking four corners...".into();
                }
            }
        }
        if attach {
            self.attach_planar();
        }
        self.ext.pl_open = open;
    }

    fn attach_planar(&mut self) {
        let Some((src_id, t0, tracks)) = self.ext.pl_data.clone() else { return };
        let (Some(src), Some(tid)) = (self.project.clip(src_id).cloned(), self.selected) else { return };
        let Some(m) = self.project.media(src.media_id).cloned() else { return };
        let Some(tgt) = self.project.clip(tid).cloned() else { return };
        if tid == src_id {
            self.ext.pl_msg = "Select the clip you want to pin (not the footage).".into();
            return;
        }
        let t_offset = (src.start + t0) - tgt.start;
        let keys = crate::tracker::to_pin_keys(&tracks, (m.width, m.height), (self.project.width, self.project.height), t_offset, src.speed, tgt.duration);
        if keys.len() < 2 {
            self.ext.pl_msg = "The tracked footage and the selected clip do not overlap in time.".into();
            return;
        }
        self.push_undo();
        let first = keys[0].pts;
        if let Some(c) = self.project.clip_mut(tid) {
            c.effects.retain(|e| !matches!(e, Effect::CornerPin { .. }));
            c.effects.push(Effect::CornerPin { pts: first, track: keys });
        }
        self.revision += 1;
        self.preview_dirty = true;
        self.ext.pl_msg = "Attached: the clip is pinned to the tracked surface (Effects > Corner pin).".into();
    }

    fn win_loudness(&mut self, ctx: &egui::Context) {
        if !self.ext.loud_open {
            return;
        }
        let mut open = true;
        let mut measure = false;
        egui::Window::new("Loudness meter").open(&mut open).default_width(300.0).show(ctx, |ui| {
            let busy = self.ext.loud_rx.is_some();
            ui.horizontal(|ui| {
                if ui.add_enabled(!busy, egui::Button::new("Measure mix")).clicked() {
                    measure = true;
                }
                if busy {
                    ui.spinner();
                }
            });
            if let Some(l) = self.ext.loud {
                egui::Grid::new("loud_grid").num_columns(2).show(ui, |ui| {
                    ui.label("Integrated");
                    ui.monospace(format!("{:.1} LUFS", l.integrated));
                    ui.end_row();
                    ui.label("Loudness range");
                    ui.monospace(format!("{:.1} LU", l.range));
                    ui.end_row();
                    ui.label("True peak");
                    ui.monospace(format!("{:.1} dBTP", l.true_peak));
                    ui.end_row();
                });
                let tip = if l.integrated > -13.0 { "Louder than most streaming targets (-14 LUFS)." } else if l.integrated < -16.0 { "Quieter than most streaming targets (-14 LUFS)." } else { "Close to the common -14 LUFS streaming target." };
                ui.label(egui::RichText::new(tip).weak().small());
            }
            if !self.ext.loud_msg.is_empty() {
                ui.label(&self.ext.loud_msg);
            }
            ui.label(egui::RichText::new("Measures the in-out range if set, else the whole timeline. Turn on loudness normalisation in Export to hit a target.").weak().small());
        });
        if measure {
            self.start_loudness(ctx);
        }
        self.ext.loud_open = open;
    }
}

/// True when two files have identical contents (streams both in 1 MiB chunks).
fn same_bytes(a: &std::path::Path, b: &std::path::Path) -> std::io::Result<bool> {
    use std::io::Read;
    let (mut fa, mut fb) = (std::fs::File::open(a)?, std::fs::File::open(b)?);
    if fa.metadata()?.len() != fb.metadata()?.len() {
        return Ok(false);
    }
    let (mut ba, mut bb) = (vec![0u8; 1 << 20], vec![0u8; 1 << 20]);
    loop {
        let n = fa.read(&mut ba)?;
        if n == 0 {
            return Ok(true);
        }
        fb.read_exact(&mut bb[..n])?;
        if ba[..n] != bb[..n] {
            return Ok(false);
        }
    }
}
