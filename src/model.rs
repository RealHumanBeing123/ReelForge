use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const EPS: f64 = 1e-6;
pub const IMAGE_DEFAULT_SECS: f64 = 5.0;

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum MediaKind {
    Video,
    Audio,
    Image,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct MediaItem {
    pub id: u64,
    pub path: PathBuf,
    pub name: String,
    pub kind: MediaKind,
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub has_audio: bool,
    pub video_codec: String,
    pub audio_codec: String,
}


// ------------------------------------------------------------------ transitions / effects / animation

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum TransitionKind {
    None,
    Fade,
    SlideLeft,
    SlideRight,
    SlideUp,
    SlideDown,
    Zoom,
}

impl TransitionKind {
    pub const ALL: [TransitionKind; 7] = [
        TransitionKind::None,
        TransitionKind::Fade,
        TransitionKind::SlideLeft,
        TransitionKind::SlideRight,
        TransitionKind::SlideUp,
        TransitionKind::SlideDown,
        TransitionKind::Zoom,
    ];
    pub fn label(self) -> &'static str {
        match self {
            TransitionKind::None => "None",
            TransitionKind::Fade => "Fade",
            TransitionKind::SlideLeft => "Slide (left)",
            TransitionKind::SlideRight => "Slide (right)",
            TransitionKind::SlideUp => "Slide (up)",
            TransitionKind::SlideDown => "Slide (down)",
            TransitionKind::Zoom => "Zoom",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize, Debug)]
pub struct Transition {
    pub kind: TransitionKind,
    pub duration: f64,
}

impl Default for Transition {
    fn default() -> Self {
        Self { kind: TransitionKind::None, duration: 0.5 }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub enum Effect {
    ColorAdjust { brightness: f32, contrast: f32, saturation: f32 },
    Blur { sigma: f32 },
    Sharpen { amount: f32 },
    Grayscale,
    Sepia,
    Invert,
    Vignette,
    FlipH,
    FlipV,
    ChromaKey { color: [u8; 3], similarity: f32, blend: f32 },
}

impl Effect {
    pub fn catalog() -> Vec<Effect> {
        vec![
            Effect::ColorAdjust { brightness: 0.0, contrast: 1.0, saturation: 1.0 },
            Effect::Blur { sigma: 6.0 },
            Effect::Sharpen { amount: 1.0 },
            Effect::Grayscale,
            Effect::Sepia,
            Effect::Invert,
            Effect::Vignette,
            Effect::FlipH,
            Effect::FlipV,
            Effect::ChromaKey { color: [0, 255, 0], similarity: 0.15, blend: 0.1 },
        ]
    }
    pub fn label(&self) -> &'static str {
        match self {
            Effect::ColorAdjust { .. } => "Color adjustment",
            Effect::Blur { .. } => "Blur",
            Effect::Sharpen { .. } => "Sharpen",
            Effect::Grayscale => "Grayscale",
            Effect::Sepia => "Sepia",
            Effect::Invert => "Invert colors",
            Effect::Vignette => "Vignette",
            Effect::FlipH => "Flip horizontal",
            Effect::FlipV => "Flip vertical",
            Effect::ChromaKey { .. } => "Remove a color (chroma key)",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize, Debug)]
pub struct Keyframe {
    /// Seconds from the start of the clip.
    pub t: f64,
    pub v: f64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AnimProp {
    Opacity,
    Scale,
    PosX,
    PosY,
}

impl AnimProp {
    pub const ALL: [AnimProp; 4] = [AnimProp::Opacity, AnimProp::Scale, AnimProp::PosX, AnimProp::PosY];
    pub fn label(self) -> &'static str {
        match self {
            AnimProp::Opacity => "Opacity",
            AnimProp::Scale => "Scale",
            AnimProp::PosX => "Position X (fraction of canvas)",
            AnimProp::PosY => "Position Y (fraction of canvas)",
        }
    }
    pub fn default_value(self) -> f64 {
        match self {
            AnimProp::Opacity | AnimProp::Scale => 1.0,
            _ => 0.0,
        }
    }
    pub fn range(self) -> std::ops::RangeInclusive<f64> {
        match self {
            AnimProp::Opacity => 0.0..=1.0,
            AnimProp::Scale => 0.1..=4.0,
            _ => -1.5..=1.5,
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug, Default)]
pub struct Anim {
    pub opacity: Vec<Keyframe>,
    pub scale: Vec<Keyframe>,
    pub pos_x: Vec<Keyframe>,
    pub pos_y: Vec<Keyframe>,
}

impl Anim {
    pub fn track(&self, p: AnimProp) -> &Vec<Keyframe> {
        match p {
            AnimProp::Opacity => &self.opacity,
            AnimProp::Scale => &self.scale,
            AnimProp::PosX => &self.pos_x,
            AnimProp::PosY => &self.pos_y,
        }
    }
    pub fn track_mut(&mut self, p: AnimProp) -> &mut Vec<Keyframe> {
        match p {
            AnimProp::Opacity => &mut self.opacity,
            AnimProp::Scale => &mut self.scale,
            AnimProp::PosX => &mut self.pos_x,
            AnimProp::PosY => &mut self.pos_y,
        }
    }
    pub fn is_empty(&self) -> bool {
        AnimProp::ALL.iter().all(|p| self.track(*p).is_empty())
    }
    pub fn all_times(&self) -> Vec<f64> {
        AnimProp::ALL.iter().flat_map(|p| self.track(*p).iter().map(|k| k.t)).collect()
    }
    /// Split into (left, right) at clip-relative time `at`, keeping values continuous.
    pub fn split_at(&self, at: f64) -> (Anim, Anim) {
        let mut l = Anim::default();
        let mut r = Anim::default();
        for p in AnimProp::ALL {
            let (a, b) = split_keyframes(self.track(p), at, p.default_value());
            *l.track_mut(p) = a;
            *r.track_mut(p) = b;
        }
        (l, r)
    }
}

/// Piecewise-linear value of a keyframe track at time `t`.
pub fn sample_keyframes(kfs: &[Keyframe], t: f64, default: f64) -> f64 {
    if kfs.is_empty() {
        return default;
    }
    if t <= kfs[0].t {
        return kfs[0].v;
    }
    for w in kfs.windows(2) {
        if t <= w[1].t {
            let span = (w[1].t - w[0].t).max(1e-9);
            return w[0].v + (w[1].v - w[0].v) * (t - w[0].t) / span;
        }
    }
    kfs[kfs.len() - 1].v
}

fn split_keyframes(kfs: &[Keyframe], at: f64, default: f64) -> (Vec<Keyframe>, Vec<Keyframe>) {
    if kfs.is_empty() {
        return (vec![], vec![]);
    }
    let v = sample_keyframes(kfs, at, default);
    let mut left: Vec<Keyframe> = kfs.iter().copied().filter(|k| k.t < at).collect();
    left.push(Keyframe { t: at, v });
    let mut right = vec![Keyframe { t: 0.0, v }];
    right.extend(kfs.iter().filter(|k| k.t > at).map(|k| Keyframe { t: k.t - at, v: k.v }));
    (left, right)
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum TitleFont {
    SegoeUi,
    Arial,
    Georgia,
    Impact,
    Consolas,
}

impl TitleFont {
    pub const ALL: [TitleFont; 5] = [TitleFont::SegoeUi, TitleFont::Arial, TitleFont::Georgia, TitleFont::Impact, TitleFont::Consolas];
    pub fn label(self) -> &'static str {
        match self {
            TitleFont::SegoeUi => "Segoe UI",
            TitleFont::Arial => "Arial",
            TitleFont::Georgia => "Georgia",
            TitleFont::Impact => "Impact",
            TitleFont::Consolas => "Consolas",
        }
    }
    pub fn file(self) -> &'static str {
        match self {
            TitleFont::SegoeUi => "segoeui.ttf",
            TitleFont::Arial => "arial.ttf",
            TitleFont::Georgia => "georgia.ttf",
            TitleFont::Impact => "impact.ttf",
            TitleFont::Consolas => "consola.ttf",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum TitlePos {
    Center,
    LowerThird,
    Top,
}

impl TitlePos {
    pub const ALL: [TitlePos; 3] = [TitlePos::Center, TitlePos::LowerThird, TitlePos::Top];
    pub fn label(self) -> &'static str {
        match self {
            TitlePos::Center => "Center",
            TitlePos::LowerThird => "Lower third",
            TitlePos::Top => "Top",
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct TitleSpec {
    pub text: String,
    pub font: TitleFont,
    /// Font size as a percentage of canvas height.
    pub size: f32,
    pub color: [u8; 3],
    pub position: TitlePos,
    pub background: bool,
    /// Custom centre point as a fraction of the canvas (0..1). Overrides `position` when set.
    #[serde(default)]
    pub pos: Option<[f32; 2]>,
    /// Box size as a fraction of the canvas; 0 means "fit the text".
    #[serde(default)]
    pub box_w: f32,
    #[serde(default)]
    pub box_h: f32,
    #[serde(default)]
    pub bg_color: [u8; 3],
    #[serde(default = "default_bg_opacity")]
    pub bg_opacity: f32,
}

fn default_bg_opacity() -> f32 {
    0.55
}

impl Default for TitleSpec {
    fn default() -> Self {
        Self {
            text: "Your title".to_string(),
            font: TitleFont::SegoeUi,
            size: 8.0,
            color: [255, 255, 255],
            position: TitlePos::Center,
            background: false,
            pos: None,
            box_w: 0.0,
            box_h: 0.0,
            bg_color: [0, 0, 0],
            bg_opacity: 1.0,
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Clip {
    pub id: u64,
    pub media_id: u64,
    pub track: usize,
    /// Position on the timeline (seconds).
    pub start: f64,
    /// Offset into the source media (seconds). Always 0 for images.
    pub in_point: f64,
    pub duration: f64,
    pub volume: f32,
    /// Audio fade lengths in seconds.
    #[serde(default)]
    pub audio_fade_in: f64,
    #[serde(default)]
    pub audio_fade_out: f64,
    #[serde(default)]
    pub transition_in: Transition,
    #[serde(default)]
    pub transition_out: Transition,
    #[serde(default)]
    pub effects: Vec<Effect>,
    #[serde(default)]
    pub anim: Anim,
    /// Generated text clip. `media_id` is 0 for these.
    #[serde(default)]
    pub title: Option<TitleSpec>,
    /// Runtime only: seconds already elapsed when a playback window starts inside this clip.
    #[serde(skip)]
    pub anim_offset: f64,
}

impl Clip {
    pub fn end(&self) -> f64 {
        self.start + self.duration
    }

    pub fn new(id: u64, media_id: u64, track: usize, start: f64, in_point: f64, duration: f64) -> Self {
        Self {
            id,
            media_id,
            track,
            start,
            in_point,
            duration,
            volume: 1.0,
            audio_fade_in: 0.0,
            audio_fade_out: 0.0,
            transition_in: Transition::default(),
            transition_out: Transition::default(),
            effects: vec![],
            anim: Anim::default(),
            title: None,
            anim_offset: 0.0,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Track {
    pub name: String,
    pub muted: bool,
    pub hidden: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Project {
    pub media: Vec<MediaItem>,
    /// Index 0 is the bottom layer; the highest index is drawn on top.
    pub tracks: Vec<Track>,
    pub clips: Vec<Clip>,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub next_id: u64,
}

impl Default for Project {
    fn default() -> Self {
        Self {
            media: vec![],
            tracks: (1..=3)
                .map(|i| Track { name: format!("Track {i}"), muted: false, hidden: false })
                .collect(),
            clips: vec![],
            width: 1920,
            height: 1080,
            fps: 30,
            next_id: 1,
        }
    }
}

impl Project {
    pub fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn media(&self, id: u64) -> Option<&MediaItem> {
        self.media.iter().find(|m| m.id == id)
    }

    pub fn clip(&self, id: u64) -> Option<&Clip> {
        self.clips.iter().find(|c| c.id == id)
    }

    pub fn clip_mut(&mut self, id: u64) -> Option<&mut Clip> {
        self.clips.iter_mut().find(|c| c.id == id)
    }

    pub fn duration(&self) -> f64 {
        self.clips.iter().map(|c| c.end()).fold(0.0, f64::max)
    }

    pub fn add_track(&mut self) {
        let n = self.tracks.len() + 1;
        self.tracks.push(Track { name: format!("Track {n}"), muted: false, hidden: false });
    }

    pub fn overlaps(&self, track: usize, start: f64, dur: f64, ignore: Option<u64>) -> bool {
        self.clips.iter().any(|c| {
            c.track == track
                && Some(c.id) != ignore
                && c.start < start + dur - EPS
                && c.end() > start + EPS
        })
    }

    /// Earliest start >= `start` on `track` where a clip of `dur` fits.
    pub fn free_start(&self, track: usize, start: f64, dur: f64, ignore: Option<u64>) -> f64 {
        let mut s = start.max(0.0);
        loop {
            let hit = self
                .clips
                .iter()
                .filter(|c| {
                    c.track == track
                        && Some(c.id) != ignore
                        && c.start < s + dur - EPS
                        && c.end() > s + EPS
                })
                .map(|c| c.end())
                .fold(None, |a: Option<f64>, e| Some(a.map_or(e, |x| x.max(e))));
            match hit {
                Some(e) => s = e,
                None => return s,
            }
        }
    }

    /// Closest start to `desired` where a clip of `dur` fits on `track` without overlapping.
    pub fn nearest_free(&self, track: usize, desired: f64, dur: f64, ignore: Option<u64>) -> f64 {
        let desired = desired.max(0.0);
        if !self.overlaps(track, desired, dur, ignore) {
            return desired;
        }
        let mut best: Option<f64> = None;
        for c in self.clips.iter().filter(|c| c.track == track && Some(c.id) != ignore) {
            for cand in [c.end(), c.start - dur] {
                if cand >= 0.0 && !self.overlaps(track, cand, dur, ignore) && best.map_or(true, |b| (cand - desired).abs() < (b - desired).abs()) {
                    best = Some(cand);
                }
            }
        }
        best.unwrap_or_else(|| self.free_start(track, desired, dur, ignore))
    }

    /// End of the nearest clip on `track` that finishes at or before `start` (0 if none).
    pub fn gap_before(&self, track: usize, start: f64, ignore: Option<u64>) -> f64 {
        self.clips
            .iter()
            .filter(|c| c.track == track && Some(c.id) != ignore && c.end() <= start + EPS)
            .map(|c| c.end())
            .fold(0.0, f64::max)
    }

    /// Start of the nearest clip on `track` that begins at or after `end` (infinity if none).
    pub fn gap_after(&self, track: usize, end: f64, ignore: Option<u64>) -> f64 {
        self.clips
            .iter()
            .filter(|c| c.track == track && Some(c.id) != ignore && c.start >= end - EPS)
            .map(|c| c.start)
            .fold(f64::INFINITY, f64::min)
    }

    pub fn add_clip(&mut self, media_id: u64, track: usize, start: f64) -> Option<u64> {
        let dur = self.media(media_id)?.duration;
        let track = track.min(self.tracks.len().saturating_sub(1));
        let start = self.nearest_free(track, start, dur, None);
        let id = self.alloc_id();
        self.clips.push(Clip::new(id, media_id, track, start, 0.0, dur));
        Some(id)
    }

    pub fn add_title(&mut self, track: usize, start: f64, spec: TitleSpec) -> u64 {
        let track = track.min(self.tracks.len().saturating_sub(1));
        let dur = 4.0;
        let start = self.nearest_free(track, start, dur, None);
        let id = self.alloc_id();
        let mut c = Clip::new(id, 0, track, start, 0.0, dur);
        c.title = Some(spec);
        c.transition_in = Transition { kind: TransitionKind::Fade, duration: 0.5 };
        c.transition_out = Transition { kind: TransitionKind::Fade, duration: 0.5 };
        self.clips.push(c);
        id
    }

    /// Split a clip at timeline time `t`. Returns the id of the new right-hand clip.
    pub fn split(&mut self, clip_id: u64, t: f64) -> Option<u64> {
        let c = self.clip(clip_id)?.clone();
        if t <= c.start + 0.05 || t >= c.end() - 0.05 {
            return None;
        }
        let is_image = self.media(c.media_id).map_or(false, |m| m.kind == MediaKind::Image);
        let first = t - c.start;
        let new_id = self.alloc_id();
        let (anim_l, anim_r) = c.anim.split_at(first);
        {
            let left = self.clip_mut(clip_id)?;
            left.duration = first;
            left.anim = anim_l;
            left.transition_out = Transition::default();
            left.audio_fade_out = 0.0;
        }
        let mut right = Clip::new(new_id, c.media_id, c.track, t, if is_image { 0.0 } else { c.in_point + first }, c.duration - first);
        right.volume = c.volume;
        right.effects = c.effects.clone();
        right.title = c.title.clone();
        right.anim = anim_r;
        right.transition_out = c.transition_out;
        right.audio_fade_out = c.audio_fade_out;
        self.clips.push(right);
        Some(new_id)
    }

    pub fn remove_clip(&mut self, id: u64) {
        self.clips.retain(|c| c.id != id);
    }

    pub fn remove_media(&mut self, id: u64) {
        self.clips.retain(|c| c.media_id != id);
        self.media.retain(|m| m.id != id);
    }

    /// A copy of the project that starts at timeline time `t0` (clips before it are dropped
    /// or cropped), used to start playback from the playhead.
    pub fn window_from(&self, t0: f64) -> Project {
        let mut p = self.clone();
        p.clips = self
            .clips
            .iter()
            .filter(|c| c.end() > t0 + EPS)
            .map(|c| {
                let cut = (t0 - c.start).max(0.0);
                let is_image = self.media(c.media_id).map_or(false, |m| m.kind == MediaKind::Image);
                let mut n = c.clone();
                n.start = (c.start - t0).max(0.0);
                n.in_point = if is_image { 0.0 } else { c.in_point + cut };
                n.duration = c.duration - cut;
                n.anim_offset = cut;
                n
            })
            .collect();
        p
    }

    /// Output dimensions for a "short side" resolution preset, keeping the canvas aspect ratio.
    pub fn output_size(&self, short_side: u32) -> (u32, u32) {
        let (pw, ph) = (self.width as f64, self.height as f64);
        let even = |v: f64| ((v / 2.0).round() as u32 * 2).max(2);
        if pw >= ph {
            (even(short_side as f64 * pw / ph), even(short_side as f64))
        } else {
            (even(short_side as f64), even(short_side as f64 * ph / pw))
        }
    }
}
