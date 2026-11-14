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
    /// Comma-separated tags for searching the media bin.
    #[serde(default)]
    pub tag: String,
    /// Bin (folder) name; empty = no bin.
    #[serde(default)]
    pub bin: String,
    /// Lightweight proxy file used for preview when proxies are on.
    #[serde(default)]
    pub proxy: Option<PathBuf>,
    /// File details recorded at import.
    #[serde(default)]
    pub info: MediaInfo,
}

/// Details of the source file, recorded when it is imported.
#[derive(Clone, PartialEq, Serialize, Deserialize, Debug, Default)]
pub struct MediaInfo {
    pub size_bytes: u64,
    pub fps: f64,
    pub bitrate_kbps: u32,
    /// File modification time, seconds since the Unix epoch.
    pub modified: u64,
    /// LUT applied automatically to clips made from this media (empty = none).
    pub lut: String,
}

/// One line of the ingest log: what was imported, and when.
#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct IngestEntry {
    pub when: u64,
    pub name: String,
    pub path: String,
    pub kind: String,
    pub duration: f64,
    pub size_bytes: u64,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Marker {
    pub t: f64,
    pub name: String,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Subtitle {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug, Default)]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Add,
    Darken,
    Lighten,
    Difference,
    SoftLight,
    HardLight,
}

impl BlendMode {
    pub const ALL: [BlendMode; 10] = [
        BlendMode::Normal,
        BlendMode::Multiply,
        BlendMode::Screen,
        BlendMode::Overlay,
        BlendMode::Add,
        BlendMode::Darken,
        BlendMode::Lighten,
        BlendMode::Difference,
        BlendMode::SoftLight,
        BlendMode::HardLight,
    ];
    pub fn label(self) -> &'static str {
        match self {
            BlendMode::Normal => "Normal",
            BlendMode::Multiply => "Multiply",
            BlendMode::Screen => "Screen",
            BlendMode::Overlay => "Overlay",
            BlendMode::Add => "Add",
            BlendMode::Darken => "Darken",
            BlendMode::Lighten => "Lighten",
            BlendMode::Difference => "Difference",
            BlendMode::SoftLight => "Soft light",
            BlendMode::HardLight => "Hard light",
        }
    }
    /// FFmpeg `blend` filter mode name.
    pub fn ff(self) -> &'static str {
        match self {
            BlendMode::Normal => "normal",
            BlendMode::Multiply => "multiply",
            BlendMode::Screen => "screen",
            BlendMode::Overlay => "overlay",
            BlendMode::Add => "addition",
            BlendMode::Darken => "darken",
            BlendMode::Lighten => "lighten",
            BlendMode::Difference => "difference",
            BlendMode::SoftLight => "softlight",
            BlendMode::HardLight => "hardlight",
        }
    }
}

/// Per-clip audio processing, applied before volume and fades.
#[derive(Clone, PartialEq, Serialize, Deserialize, Debug, Default)]
pub struct AudioFx {
    /// -1 (left) .. 1 (right).
    pub pan: f32,
    /// 3-band EQ gains in dB.
    pub eq_low: f32,
    pub eq_mid: f32,
    pub eq_high: f32,
    /// 0..1 compressor amount.
    pub compress: f32,
    /// 0..1 noise reduction amount.
    pub denoise: f32,
    /// 0..1 de-esser amount.
    pub deess: f32,
    /// 0..1 reverb mix.
    pub reverb: f32,
    /// Echo / delay time in ms (0 = off) and feedback 0..0.9.
    pub echo_ms: f32,
    pub echo_decay: f32,
    /// Per-clip loudness normalisation (single pass).
    pub normalize: bool,
}

impl AudioFx {
    pub fn is_default(&self) -> bool {
        *self == AudioFx::default()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug, Default)]
pub enum HslBand {
    #[default]
    All,
    Red,
    Yellow,
    Green,
    Cyan,
    Blue,
    Magenta,
}

impl HslBand {
    pub const ALL: [HslBand; 7] = [HslBand::All, HslBand::Red, HslBand::Yellow, HslBand::Green, HslBand::Cyan, HslBand::Blue, HslBand::Magenta];
    pub fn label(self) -> &'static str {
        match self {
            HslBand::All => "All colors",
            HslBand::Red => "Reds",
            HslBand::Yellow => "Yellows",
            HslBand::Green => "Greens",
            HslBand::Cyan => "Cyans",
            HslBand::Blue => "Blues",
            HslBand::Magenta => "Magentas",
        }
    }
    pub fn ff(self) -> &'static str {
        match self {
            HslBand::All => "r+y+g+c+b+m",
            HslBand::Red => "r",
            HslBand::Yellow => "y",
            HslBand::Green => "g",
            HslBand::Cyan => "c",
            HslBand::Blue => "b",
            HslBand::Magenta => "m",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug, Default)]
pub enum CurvePreset {
    #[default]
    IncreaseContrast,
    StrongContrast,
    Lighter,
    Darker,
    Vintage,
    CrossProcess,
    ColorNegative,
}

impl CurvePreset {
    pub const ALL: [CurvePreset; 7] = [
        CurvePreset::IncreaseContrast,
        CurvePreset::StrongContrast,
        CurvePreset::Lighter,
        CurvePreset::Darker,
        CurvePreset::Vintage,
        CurvePreset::CrossProcess,
        CurvePreset::ColorNegative,
    ];
    pub fn label(self) -> &'static str {
        match self {
            CurvePreset::IncreaseContrast => "Increase contrast",
            CurvePreset::StrongContrast => "Strong contrast",
            CurvePreset::Lighter => "Lighter",
            CurvePreset::Darker => "Darker",
            CurvePreset::Vintage => "Vintage",
            CurvePreset::CrossProcess => "Cross process",
            CurvePreset::ColorNegative => "Color negative",
        }
    }
    pub fn ff(self) -> &'static str {
        match self {
            CurvePreset::IncreaseContrast => "increase_contrast",
            CurvePreset::StrongContrast => "strong_contrast",
            CurvePreset::Lighter => "lighter",
            CurvePreset::Darker => "darker",
            CurvePreset::Vintage => "vintage",
            CurvePreset::CrossProcess => "cross_process",
            CurvePreset::ColorNegative => "color_negative",
        }
    }
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
    /// Animated zoom from `from` to `to` (1.0 = normal) over the clip, centred on (fx, fy) in 0..1.
    Zoom { from: f32, to: f32, fx: f32, fy: f32, ease: bool },
    /// Lift / gamma / gain as per-channel (R, G, B) offsets in -1..1 for shadows / mids / highlights.
    ColorWheels { lift: [f32; 3], gamma: [f32; 3], gain: [f32; 3] },
    /// Colour temperature in Kelvin (6500 = neutral) and green-magenta tint (-1..1).
    Temperature { kelvin: f32, tint: f32 },
    /// 3D LUT (.cube / .3dl / .dat / .m3d / .csp).
    Lut { path: String },
    /// Secondary grade of one colour band: hue shift (deg), saturation and intensity (-1..1).
    HslAdjust { band: HslBand, hue: f32, sat: f32, intensity: f32 },
    Curves { preset: CurvePreset },
    Glow { radius: f32, strength: f32 },
    /// Lens distortion: negative = pincushion, positive = barrel.
    Distort { amount: f32 },
    Rotate { degrees: f32 },
    Pixelate { block: f32 },
    Grain { amount: f32 },
    Denoise { strength: f32 },
    Stabilize { strength: f32 },
    /// Soft-edged rectangle or ellipse mask (all values as fractions of the picture).
    Mask { ellipse: bool, cx: f32, cy: f32, w: f32, h: f32, feather: f32, invert: bool },
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
            Effect::Zoom { from: 1.0, to: 1.5, fx: 0.5, fy: 0.5, ease: true },
            Effect::Zoom { from: 1.5, to: 1.0, fx: 0.5, fy: 0.5, ease: true },
            Effect::ColorWheels { lift: [0.0; 3], gamma: [0.0; 3], gain: [0.0; 3] },
            Effect::Temperature { kelvin: 6500.0, tint: 0.0 },
            Effect::Lut { path: String::new() },
            Effect::HslAdjust { band: HslBand::Red, hue: 0.0, sat: 0.0, intensity: 0.0 },
            Effect::Curves { preset: CurvePreset::IncreaseContrast },
            Effect::Glow { radius: 25.0, strength: 0.6 },
            Effect::Distort { amount: 0.2 },
            Effect::Rotate { degrees: 10.0 },
            Effect::Pixelate { block: 16.0 },
            Effect::Grain { amount: 20.0 },
            Effect::Denoise { strength: 4.0 },
            Effect::Stabilize { strength: 16.0 },
            Effect::Mask { ellipse: true, cx: 0.5, cy: 0.5, w: 0.6, h: 0.6, feather: 0.15, invert: false },
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
            Effect::Zoom { from, to, .. } => {
                if from <= to {
                    "Zoom in"
                } else {
                    "Zoom out"
                }
            }
            Effect::ColorWheels { .. } => "Color wheels (lift / gamma / gain)",
            Effect::Temperature { .. } => "Temperature & tint",
            Effect::Lut { .. } => "LUT",
            Effect::HslAdjust { .. } => "HSL secondary",
            Effect::Curves { .. } => "Curves preset",
            Effect::Glow { .. } => "Glow",
            Effect::Distort { .. } => "Lens distort / warp",
            Effect::Rotate { .. } => "Rotate",
            Effect::Pixelate { .. } => "Pixelate",
            Effect::Grain { .. } => "Film grain",
            Effect::Denoise { .. } => "Video denoise",
            Effect::Stabilize { .. } => "Stabilize",
            Effect::Mask { .. } => "Mask (shape, feather)",
        }
    }
}

/// How a value moves from one keyframe to the next.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug, Default)]
pub enum Ease {
    #[default]
    Linear,
    In,
    Out,
    InOut,
    Hold,
}

impl Ease {
    pub const ALL: [Ease; 5] = [Ease::Linear, Ease::In, Ease::Out, Ease::InOut, Ease::Hold];
    pub fn label(self) -> &'static str {
        match self {
            Ease::Linear => "Linear",
            Ease::In => "Ease in",
            Ease::Out => "Ease out",
            Ease::InOut => "Ease in-out",
            Ease::Hold => "Hold",
        }
    }
    /// Map progress 0..1 through the curve.
    pub fn apply(self, p: f64) -> f64 {
        let p = p.clamp(0.0, 1.0);
        match self {
            Ease::Linear => p,
            Ease::In => p * p,
            Ease::Out => 1.0 - (1.0 - p) * (1.0 - p),
            Ease::InOut => p * p * (3.0 - 2.0 * p),
            Ease::Hold => 0.0,
        }
    }
    /// FFmpeg expression for the curve applied to the expression `p` (0..1).
    pub fn expr(self, p: &str) -> String {
        match self {
            Ease::Linear => p.to_string(),
            Ease::In => format!("({p})*({p})"),
            Ease::Out => format!("(1-(1-({p}))*(1-({p})))"),
            Ease::InOut => format!("(({p})*({p})*(3-2*({p})))"),
            Ease::Hold => "0".to_string(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize, Debug)]
pub struct Keyframe {
    /// Seconds from the start of the clip.
    pub t: f64,
    pub v: f64,
    /// Easing of the segment that starts at this keyframe.
    #[serde(default)]
    pub ease: Ease,
}

impl Keyframe {
    pub fn new(t: f64, v: f64) -> Self {
        Self { t, v, ease: Ease::Linear }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AnimProp {
    Opacity,
    Scale,
    PosX,
    PosY,
    Volume,
}

impl AnimProp {
    pub const ALL: [AnimProp; 5] = [AnimProp::Opacity, AnimProp::Scale, AnimProp::PosX, AnimProp::PosY, AnimProp::Volume];
    pub fn label(self) -> &'static str {
        match self {
            AnimProp::Opacity => "Opacity",
            AnimProp::Scale => "Scale",
            AnimProp::PosX => "Position X (fraction of canvas)",
            AnimProp::PosY => "Position Y (fraction of canvas)",
            AnimProp::Volume => "Audio level (1 = clip volume)",
        }
    }
    pub fn default_value(self) -> f64 {
        match self {
            AnimProp::Opacity | AnimProp::Scale | AnimProp::Volume => 1.0,
            _ => 0.0,
        }
    }
    pub fn range(self) -> std::ops::RangeInclusive<f64> {
        match self {
            AnimProp::Opacity => 0.0..=1.0,
            AnimProp::Scale => 0.1..=4.0,
            AnimProp::Volume => 0.0..=2.0,
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
    #[serde(default)]
    pub volume: Vec<Keyframe>,
}

impl Anim {
    pub fn track(&self, p: AnimProp) -> &Vec<Keyframe> {
        match p {
            AnimProp::Opacity => &self.opacity,
            AnimProp::Scale => &self.scale,
            AnimProp::PosX => &self.pos_x,
            AnimProp::PosY => &self.pos_y,
            AnimProp::Volume => &self.volume,
        }
    }
    pub fn track_mut(&mut self, p: AnimProp) -> &mut Vec<Keyframe> {
        match p {
            AnimProp::Opacity => &mut self.opacity,
            AnimProp::Scale => &mut self.scale,
            AnimProp::PosX => &mut self.pos_x,
            AnimProp::PosY => &mut self.pos_y,
            AnimProp::Volume => &mut self.volume,
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
            return w[0].v + (w[1].v - w[0].v) * w[0].ease.apply((t - w[0].t) / span);
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
    let seg_ease = kfs.iter().filter(|k| k.t <= at).last().map(|k| k.ease).unwrap_or_default();
    left.push(Keyframe { t: at, v, ease: seg_ease });
    let mut right = vec![Keyframe { t: 0.0, v, ease: seg_ease }];
    right.extend(kfs.iter().filter(|k| k.t > at).map(|k| Keyframe { t: k.t - at, v: k.v, ease: k.ease }));
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

fn one() -> f64 {
    1.0
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
    /// Playback speed (1.0 = normal). The clip covers `duration * speed` seconds of source.
    #[serde(default = "one")]
    pub speed: f64,
    /// Play the source backwards.
    #[serde(default)]
    pub reverse: bool,
    /// Freeze frame: holds the single source frame at `in_point` for `duration`.
    #[serde(default)]
    pub freeze: bool,
    /// Layer blend mode against what is beneath it.
    #[serde(default)]
    pub blend: BlendMode,
    /// Smooth slow motion by frame interpolation (slow; only used when speed < 1).
    #[serde(default)]
    pub interpolate: bool,
    #[serde(default)]
    pub afx: AudioFx,
    /// Runtime only: seconds already elapsed when a playback window starts inside this clip.
    #[serde(skip)]
    pub anim_offset: f64,
}

impl Clip {
    pub fn end(&self) -> f64 {
        self.start + self.duration
    }

    /// Source-time seconds consumed by the clip.
    pub fn src_span(&self) -> f64 {
        self.duration * self.speed
    }

    /// Source time shown at `x` seconds into the clip.
    pub fn src_time_at(&self, x: f64) -> f64 {
        if self.freeze {
            self.in_point
        } else if self.reverse {
            self.in_point + (self.duration - x).max(0.0) * self.speed
        } else {
            self.in_point + x * self.speed
        }
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
            speed: 1.0,
            reverse: false,
            freeze: false,
            blend: BlendMode::Normal,
            interpolate: false,
            afx: AudioFx::default(),
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
    #[serde(default)]
    pub markers: Vec<Marker>,
    #[serde(default)]
    pub mark_in: Option<f64>,
    #[serde(default)]
    pub mark_out: Option<f64>,
    #[serde(default)]
    pub subtitles: Vec<Subtitle>,
    /// Use proxy files (where they exist) for preview and playback. Export always uses originals.
    #[serde(default)]
    pub use_proxies: bool,
    /// Every file imported into this project.
    #[serde(default)]
    pub ingest: Vec<IngestEntry>,
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
            markers: vec![],
            mark_in: None,
            mark_out: None,
            subtitles: vec![],
            use_proxies: false,
            ingest: vec![],
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

    /// Times worth snapping to: the timeline start plus every clip start / end (all tracks).
    pub fn edge_points(&self, ignore: Option<u64>) -> Vec<f64> {
        let mut v = vec![0.0];
        v.extend(self.markers.iter().map(|m| m.t));
        v.extend(self.mark_in);
        v.extend(self.mark_out);
        for c in self.clips.iter().filter(|c| Some(c.id) != ignore) {
            v.push(c.start);
            v.push(c.end());
        }
        v
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
        self.push_media_clip(Clip::new(id, media_id, track, start, 0.0, dur));
        Some(id)
    }

    /// Add a clip made from media, applying the media's default LUT (if it has one).
    fn push_media_clip(&mut self, mut c: Clip) {
        if let Some(m) = self.media(c.media_id) {
            if !m.info.lut.is_empty() && m.kind != MediaKind::Audio {
                c.effects.push(Effect::Lut { path: m.info.lut.clone() });
            }
        }
        self.clips.push(c);
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
        let is_image = c.freeze || self.media(c.media_id).map_or(false, |m| m.kind == MediaKind::Image);
        let first = t - c.start;
        let new_id = self.alloc_id();
        let (anim_l, anim_r) = c.anim.split_at(first);
        {
            let left = self.clip_mut(clip_id)?;
            left.duration = first;
            left.anim = anim_l;
            left.transition_out = Transition::default();
            left.audio_fade_out = 0.0;
            if c.reverse && !is_image {
                // Reversed: the left part is the *later* part of the source.
                left.in_point = c.in_point + (c.duration - first) * c.speed;
            }
        }
        let right_in = if c.freeze {
            c.in_point
        } else if is_image {
            0.0
        } else if c.reverse {
            c.in_point
        } else {
            c.in_point + first * c.speed
        };
        let mut right = Clip::new(new_id, c.media_id, c.track, t, right_in, c.duration - first);
        right.speed = c.speed;
        right.reverse = c.reverse;
        right.freeze = c.freeze;
        right.blend = c.blend;
        right.interpolate = c.interpolate;
        right.afx = c.afx.clone();
        right.volume = c.volume;
        right.effects = c.effects.clone();
        right.title = c.title.clone();
        right.anim = anim_r;
        right.transition_out = c.transition_out;
        right.audio_fade_out = c.audio_fade_out;
        self.clips.push(right);
        Some(new_id)
    }

    /// Move every clip on `track` that starts at or after `from` by `delta` seconds (never below 0).
    pub fn shift_after(&mut self, track: usize, from: f64, delta: f64, ignore: Option<u64>) {
        for c in self.clips.iter_mut() {
            if c.track == track && Some(c.id) != ignore && c.start >= from - EPS {
                c.start = (c.start + delta).max(0.0);
            }
        }
    }

    /// Insert a freeze frame of the clip's picture at timeline time `t`, lasting `dur` seconds.
    /// The clip is split at `t`, later clips on the same track move right, and the frozen frame
    /// is placed in the gap. Returns the id of the freeze clip.
    pub fn insert_freeze(&mut self, clip_id: u64, t: f64, dur: f64) -> Option<u64> {
        let c = self.clip(clip_id)?.clone();
        if c.title.is_some() || c.freeze || dur <= 0.0 {
            return None;
        }
        let kind = self.media(c.media_id)?.kind;
        if kind != MediaKind::Video && kind != MediaKind::Image {
            return None;
        }
        if t < c.start - EPS || t > c.end() + EPS {
            return None;
        }
        let x = (t - c.start).clamp(0.0, c.duration);
        let src = c.src_time_at(x);
        // Snap to a cut when at an edge, otherwise split.
        let at_start = x <= 0.05;
        let at_end = x >= c.duration - 0.05;
        let freeze_start = if at_start {
            c.start
        } else if at_end {
            c.end()
        } else {
            t
        };
        let shift_from = if at_start { c.start } else { freeze_start };
        // Ripple first (before the split creates a new clip that must also move).
        self.shift_after(c.track, shift_from, dur, if at_start { None } else { Some(clip_id) });
        if !at_start && !at_end {
            let right = self.split(clip_id, t)?;
            if let Some(r) = self.clip_mut(right) {
                r.start += dur;
            }
        }
        let id = self.alloc_id();
        let mut f = Clip::new(id, c.media_id, c.track, freeze_start, src, dur);
        f.freeze = true;
        f.volume = 0.0;
        self.clips.push(f);
        Some(id)
    }

    /// Change playback speed of the part of a clip between timeline times `t0` and `t1`.
    /// Splits the clip as needed and ripples later clips on the track. Returns the middle clip id.
    pub fn apply_speed_range(&mut self, clip_id: u64, t0: f64, t1: f64, speed: f64) -> Option<u64> {
        let c = self.clip(clip_id)?.clone();
        if c.title.is_some() || c.freeze || self.media(c.media_id).map_or(true, |m| m.kind == MediaKind::Image) {
            return None;
        }
        let speed = speed.clamp(0.1, 16.0);
        let (a, b) = (t0.min(t1).max(c.start), t0.max(t1).min(c.end()));
        if b - a < 0.05 {
            return None;
        }
        let mut mid = clip_id;
        if b < c.end() - 0.05 {
            self.split(clip_id, b)?;
        }
        if a > c.start + 0.05 {
            mid = self.split(clip_id, a)?;
        }
        let m = self.clip_mut(mid)?;
        let old_d = m.duration;
        let new_d = old_d * m.speed / speed;
        m.speed = speed;
        m.duration = new_d;
        let delta = new_d - old_d;
        let bound = a + old_d;
        self.shift_after(c.track, bound, delta, Some(mid));
        Some(mid)
    }

    // ------------------------------------------------------------------ edit operations

    fn is_stills(&self, c: &Clip) -> bool {
        c.freeze || c.title.is_some() || self.media(c.media_id).map_or(true, |m| m.kind == MediaKind::Image)
    }

    /// Timeline seconds a clip can still grow at its (start edge, end edge) before running out
    /// of source media. Unlimited for images, titles and freeze frames.
    pub fn edge_room(&self, c: &Clip) -> (f64, f64) {
        if self.is_stills(c) {
            return (f64::INFINITY, f64::INFINITY);
        }
        let md = self.media(c.media_id).map_or(0.0, |m| m.duration);
        let s = c.speed.max(0.01);
        let before = c.in_point / s;
        let after = ((md - c.in_point - c.duration * s) / s).max(0.0);
        if c.reverse {
            (after, before)
        } else {
            (before, after)
        }
    }

    /// Move a clip's start edge by `delta` (timeline seconds, + shortens), keeping its end fixed.
    fn trim_start(&mut self, id: u64, delta: f64) {
        let Some(c) = self.clip(id).cloned() else { return };
        let still = self.is_stills(&c);
        if let Some(m) = self.clip_mut(id) {
            m.start = c.start + delta;
            m.duration = c.duration - delta;
            if !still && !c.reverse {
                m.in_point = c.in_point + delta * c.speed;
            }
        }
    }

    /// Move a clip's end edge by `delta` (+ lengthens), keeping its start fixed.
    fn trim_end(&mut self, id: u64, delta: f64) {
        let Some(c) = self.clip(id).cloned() else { return };
        let still = self.is_stills(&c);
        if let Some(m) = self.clip_mut(id) {
            m.duration = c.duration + delta;
            if !still && c.reverse {
                m.in_point = (c.in_point - delta * c.speed).max(0.0);
            }
        }
    }

    /// Allowed range of `delta` for a roll edit between adjacent clips `l` and `r`.
    fn roll_limits(&self, l: &Clip, r: &Clip) -> (f64, f64) {
        let (_, l_after) = self.edge_room(l);
        let (r_before, _) = self.edge_room(r);
        let hi = l_after.min(r.duration - 0.05);
        let lo = -(r_before.min(l.duration - 0.05));
        (lo, hi)
    }

    /// Roll edit: move the cut between two touching clips by `delta` seconds. Total length is
    /// unchanged. Returns the delta actually applied.
    pub fn roll(&mut self, left: u64, right: u64, delta: f64) -> f64 {
        let (Some(l), Some(r)) = (self.clip(left).cloned(), self.clip(right).cloned()) else { return 0.0 };
        if l.track != r.track || (l.end() - r.start).abs() > 1e-3 {
            return 0.0;
        }
        let (lo, hi) = self.roll_limits(&l, &r);
        let d = delta.clamp(lo.min(0.0), hi.max(0.0));
        if d.abs() < 1e-9 {
            return 0.0;
        }
        self.trim_end(left, d);
        self.trim_start(right, d);
        d
    }

    /// Slide edit: move a clip by `delta` while its neighbours absorb the change, so the total
    /// length stays the same. Returns the delta applied (0 when the clip has no neighbours).
    pub fn slide(&mut self, id: u64, delta: f64) -> f64 {
        let Some(c) = self.clip(id).cloned() else { return 0.0 };
        let prev = self.clips.iter().find(|x| x.track == c.track && x.id != id && (x.end() - c.start).abs() < 1e-3).cloned();
        let next = self.clips.iter().find(|x| x.track == c.track && x.id != id && (x.start - c.end()).abs() < 1e-3).cloned();
        if prev.is_none() && next.is_none() {
            return 0.0;
        }
        let (mut lo, mut hi) = (f64::NEG_INFINITY, f64::INFINITY);
        match (&prev, &next) {
            (Some(p), Some(n)) => {
                let (l1, h1) = self.roll_limits(p, &c);
                let (l2, h2) = self.roll_limits(&c, n);
                lo = l1.max(l2);
                hi = h1.min(h2);
            }
            (Some(p), None) => {
                let (l1, h1) = self.roll_limits(p, &c);
                lo = l1;
                hi = h1;
            }
            (None, Some(n)) => {
                // sliding right edge: c.start moves freely (gap before), n shrinks / grows
                let (l2, h2) = self.roll_limits(&c, n);
                lo = l2;
                hi = h2;
            }
            _ => {}
        }
        let d = delta.clamp(lo.min(0.0), hi.max(0.0));
        if d.abs() < 1e-9 {
            return 0.0;
        }
        if let Some(p) = &prev {
            self.trim_end(p.id, d);
        }
        if let Some(n) = &next {
            self.trim_start(n.id, d);
        }
        if let Some(m) = self.clip_mut(id) {
            m.start = c.start + d;
        }
        d
    }

    /// Slip edit: change which part of the source a clip shows without moving or resizing it.
    /// `delta` is how far the content moves along the timeline (+ = later). Returns delta applied.
    pub fn slip(&mut self, id: u64, delta: f64) -> f64 {
        let Some(c) = self.clip(id).cloned() else { return 0.0 };
        if self.is_stills(&c) {
            return 0.0;
        }
        let md = self.media(c.media_id).map_or(0.0, |m| m.duration);
        let span = c.duration * c.speed;
        let want = if c.reverse { c.in_point + delta * c.speed } else { c.in_point - delta * c.speed };
        let new_in = want.clamp(0.0, (md - span).max(0.0));
        if let Some(m) = self.clip_mut(id) {
            m.in_point = new_in;
        }
        if c.reverse {
            (new_in - c.in_point) / c.speed
        } else {
            (c.in_point - new_in) / c.speed
        }
    }

    /// Extend the nearest edge of a clip to timeline time `t` (extend edit). Stops at the next
    /// clip on the track and at the end of the source.
    pub fn extend_edge(&mut self, id: u64, t: f64) -> bool {
        let Some(c) = self.clip(id).cloned() else { return false };
        let (room_s, room_e) = self.edge_room(&c);
        let to_start = (t - c.start).abs();
        let to_end = (t - c.end()).abs();
        if to_end <= to_start {
            let limit = (self.gap_after(c.track, c.end(), Some(id)) - c.end()).min(room_e);
            let d = (t - c.end()).clamp(-(c.duration - 0.05), limit.max(0.0));
            self.trim_end(id, d);
            d.abs() > 1e-9
        } else {
            let limit = (c.start - self.gap_before(c.track, c.start, Some(id))).min(room_s).min(c.start);
            let d = (t - c.start).clamp(-limit.max(0.0), c.duration - 0.05);
            self.trim_start(id, d);
            d.abs() > 1e-9
        }
    }

    /// Delete a clip and close the gap on its track.
    pub fn ripple_delete(&mut self, id: u64) {
        let Some(c) = self.clip(id).cloned() else { return };
        self.remove_clip(id);
        self.shift_after(c.track, c.end() - EPS, -c.duration, None);
    }

    /// Remove the time range `a..b`. With `ripple`, later material (every track) closes the gap
    /// (extract); otherwise a gap is left (lift). `track` limits the cut to one track.
    pub fn cut_range(&mut self, a: f64, b: f64, ripple: bool, track: Option<usize>) {
        let (a, b) = (a.min(b), a.max(b));
        if b - a < 0.02 {
            return;
        }
        let ids: Vec<u64> = self.clips.iter().filter(|c| track.map_or(true, |t| c.track == t) && c.start < b - EPS && c.end() > a + EPS).map(|c| c.id).collect();
        for id in ids {
            let mut cur = id;
            if let Some(c) = self.clip(cur) {
                if c.end() > b + 0.05 && c.start < b - 0.05 {
                    self.split(cur, b);
                }
            }
            if let Some(c) = self.clip(cur) {
                if c.start < a - 0.05 && c.end() > a + 0.05 {
                    if let Some(r) = self.split(cur, a) {
                        cur = r;
                    }
                }
            }
            let _ = cur;
        }
        self.clips.retain(|c| !(track.map_or(true, |t| c.track == t) && c.start >= a - 0.05 && c.end() <= b + 0.05));
        if ripple {
            let d = b - a;
            for c in self.clips.iter_mut() {
                if track.map_or(true, |t| c.track == t) && c.start >= b - 0.05 {
                    c.start = (c.start - d).max(0.0);
                }
            }
        }
    }

    /// Insert a media clip at `t` on `track`, pushing later clips on the track right (insert edit).
    pub fn insert_media(&mut self, media_id: u64, track: usize, t: f64) -> Option<u64> {
        let dur = self.media(media_id)?.duration;
        let track = track.min(self.tracks.len().saturating_sub(1));
        let t = t.max(0.0);
        let hit = self.clips.iter().find(|c| c.track == track && c.start < t - 0.05 && c.end() > t + 0.05).map(|c| c.id);
        if let Some(h) = hit {
            self.split(h, t);
        }
        self.shift_after(track, t - 0.05, dur, None);
        let id = self.alloc_id();
        self.push_media_clip(Clip::new(id, media_id, track, t, 0.0, dur));
        Some(id)
    }

    /// Place a media clip at `t`, overwriting whatever is on the track there (overwrite edit).
    pub fn overwrite_media(&mut self, media_id: u64, track: usize, t: f64) -> Option<u64> {
        let dur = self.media(media_id)?.duration;
        let track = track.min(self.tracks.len().saturating_sub(1));
        let t = t.max(0.0);
        self.cut_range(t, t + dur, false, Some(track));
        let id = self.alloc_id();
        self.push_media_clip(Clip::new(id, media_id, track, t, 0.0, dur));
        Some(id)
    }

    /// Add a sub-clip of a media item (source monitor in/out) at `t`.
    pub fn add_subclip(&mut self, media_id: u64, track: usize, t: f64, src_in: f64, src_out: f64, overwrite: bool) -> Option<u64> {
        let m = self.media(media_id)?;
        let md = m.duration;
        let is_image = m.kind == MediaKind::Image;
        let (a, b) = if is_image { (0.0, src_out.max(0.1)) } else { (src_in.clamp(0.0, md), src_out.clamp(0.0, md)) };
        let dur = (b - a).max(0.05);
        let track = track.min(self.tracks.len().saturating_sub(1));
        let t = t.max(0.0);
        if overwrite {
            self.cut_range(t, t + dur, false, Some(track));
        } else {
            let t2 = self.nearest_free(track, t, dur, None);
            let id = self.alloc_id();
            self.push_media_clip(Clip::new(id, media_id, track, t2, a, dur));
            return Some(id);
        }
        let id = self.alloc_id();
        self.push_media_clip(Clip::new(id, media_id, track, t, a, dur));
        Some(id)
    }

    /// Speed ramp: change speed gradually from `s0` to `s1` across `t0..t1` of a clip, in
    /// `steps` pieces (smooth velocity curve). Returns false if the range is not usable.
    pub fn speed_ramp(&mut self, clip_id: u64, t0: f64, t1: f64, s0: f64, s1: f64, steps: usize) -> bool {
        let steps = steps.clamp(2, 40);
        let Some(mid) = self.apply_speed_range(clip_id, t0, t1, s0) else { return false };
        let Some(m) = self.clip(mid).cloned() else { return false };
        let piece = m.duration / steps as f64;
        let mut ids = vec![mid];
        let mut cur = mid;
        for k in 1..steps {
            match self.split(cur, m.start + piece * k as f64) {
                Some(r) => {
                    ids.push(r);
                    cur = r;
                }
                None => break,
            }
        }
        let n = ids.len() as f64;
        for (k, id) in ids.iter().enumerate() {
            let p = (k as f64 + 0.5) / n;
            let e = p * p * (3.0 - 2.0 * p);
            let sp = (s0 + (s1 - s0) * e).clamp(0.1, 16.0);
            let Some(c) = self.clip(*id).cloned() else { continue };
            let new_d = c.duration * c.speed / sp;
            let delta = new_d - c.duration;
            if let Some(x) = self.clip_mut(*id) {
                x.speed = sp;
                x.duration = new_d;
            }
            self.shift_after(c.track, c.end() - EPS, delta, Some(*id));
        }
        true
    }

    /// Fade the audio across the cut between a clip and the next one on its track.
    pub fn audio_crossfade(&mut self, id: u64, secs: f64) -> bool {
        let Some(c) = self.clip(id).cloned() else { return false };
        let Some(n) = self.clips.iter().find(|x| x.track == c.track && x.id != id && (x.start - c.end()).abs() < 1e-3).cloned() else { return false };
        let d = secs.min(c.duration / 2.0).min(n.duration / 2.0).max(0.05);
        if let Some(x) = self.clip_mut(id) {
            x.audio_fade_out = d;
        }
        if let Some(x) = self.clip_mut(n.id) {
            x.audio_fade_in = d;
        }
        true
    }

    pub fn add_marker(&mut self, t: f64, name: String) {
        if self.markers.iter().any(|m| (m.t - t).abs() < 0.02) {
            return;
        }
        self.markers.push(Marker { t, name });
        self.markers.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap());
    }

    /// Closest marker time strictly after / before `t`.
    pub fn next_marker(&self, t: f64) -> Option<f64> {
        self.markers.iter().map(|m| m.t).find(|x| *x > t + 0.02)
    }
    pub fn prev_marker(&self, t: f64) -> Option<f64> {
        self.markers.iter().rev().map(|m| m.t).find(|x| *x < t - 0.02)
    }

    /// Copy of the project covering only `a..b` (for exporting between the in and out points).
    pub fn range(&self, a: f64, b: f64) -> Project {
        let mut p = self.window_from(a);
        let len = (b - a).max(0.05);
        p.clips.retain(|c| c.start < len - EPS);
        for c in p.clips.iter_mut() {
            if c.end() > len {
                let cut = c.end() - len;
                let still = c.freeze || c.title.is_some() || self.media(c.media_id).map_or(true, |m| m.kind == MediaKind::Image);
                if c.reverse && !still {
                    c.in_point += cut * c.speed;
                }
                c.duration = len - c.start;
            }
        }
        p.subtitles.retain(|s| s.start < len);
        p.mark_in = None;
        p.mark_out = None;
        p
    }

    /// Replace media paths with proxies (preview only) when proxies are on.
    pub fn with_proxies(&self) -> Project {
        let mut p = self.clone();
        if p.use_proxies {
            for m in p.media.iter_mut() {
                if let Some(px) = &m.proxy {
                    if px.exists() && m.kind == MediaKind::Video {
                        m.path = px.clone();
                    }
                }
            }
        }
        p
    }

    /// A copy played back `rate` times faster (for shuttle playback): every clip starts and ends
    /// sooner and runs at `rate` times its own speed. Preview use only.
    pub fn scaled(&self, rate: f64) -> Project {
        let mut p = self.clone();
        for c in p.clips.iter_mut() {
            c.start /= rate;
            c.duration /= rate;
            c.speed *= rate;
        }
        p
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
                let is_image = c.freeze || self.media(c.media_id).map_or(false, |m| m.kind == MediaKind::Image);
                let mut n = c.clone();
                n.start = (c.start - t0).max(0.0);
                n.in_point = if c.freeze {
                    c.in_point
                } else if is_image {
                    0.0
                } else if c.reverse {
                    c.in_point
                } else {
                    c.in_point + cut * c.speed
                };
                n.duration = c.duration - cut;
                n.anim_offset = cut;
                n
            })
            .collect();
        p.subtitles = self
            .subtitles
            .iter()
            .filter(|s| s.end > t0 + EPS)
            .map(|s| Subtitle { start: (s.start - t0).max(0.0), end: s.end - t0, text: s.text.clone() })
            .collect();
        p.markers.clear();
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

/// The point closest to `t` that lies within `thr` of it, if any.
pub fn nearest_within(points: impl IntoIterator<Item = f64>, t: f64, thr: f64) -> Option<f64> {
    let mut best: Option<f64> = None;
    for e in points {
        let d = (e - t).abs();
        if d <= thr && best.map_or(true, |b| d < (b - t).abs()) {
            best = Some(e);
        }
    }
    best
}
