use crate::interchange;
use crate::model::{AudioFx, BlendMode, Clip, Effect, Keyframe, MediaKind, Project, TitlePos, TitleSpec, Transition, TransitionKind};
use crate::util::command;
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum Container {
    Mp4,
    Mkv,
    Mov,
    WebM,
}

impl Container {
    pub const ALL: [Container; 4] = [Container::Mp4, Container::Mkv, Container::Mov, Container::WebM];
    pub fn ext(self) -> &'static str {
        match self {
            Container::Mp4 => "mp4",
            Container::Mkv => "mkv",
            Container::Mov => "mov",
            Container::WebM => "webm",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Container::Mp4 => "MP4",
            Container::Mkv => "MKV",
            Container::Mov => "MOV",
            Container::WebM => "WebM",
        }
    }
    pub fn video_codecs(self) -> &'static [VideoCodec] {
        use VideoCodec::*;
        match self {
            Container::Mp4 => &[H264, H265, Av1, H264Nvenc, HevcNvenc, H264Amf, HevcAmf, H264Qsv, HevcQsv],
            Container::Mkv => &[H264, H265, Vp9, Av1, H264Nvenc, HevcNvenc, H264Amf, HevcAmf, H264Qsv, HevcQsv],
            Container::Mov => &[H264, H265, ProRes, H264Nvenc, HevcNvenc, H264Amf, HevcAmf, H264Qsv, HevcQsv],
            Container::WebM => &[Vp9, Av1],
        }
    }
    pub fn audio_codecs(self) -> &'static [AudioCodec] {
        use AudioCodec::*;
        match self {
            Container::Mp4 => &[Aac, Mp3, Opus],
            Container::Mkv => &[Aac, Mp3, Opus, Vorbis, Flac, Pcm],
            Container::Mov => &[Aac, Pcm],
            Container::WebM => &[Opus, Vorbis],
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum VideoCodec {
    H264,
    H265,
    Vp9,
    Av1,
    ProRes,
    H264Nvenc,
    HevcNvenc,
    H264Amf,
    HevcAmf,
    H264Qsv,
    HevcQsv,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum Speed {
    Fast,
    Balanced,
    Quality,
}

impl Speed {
    pub const ALL: [Speed; 3] = [Speed::Fast, Speed::Balanced, Speed::Quality];
    pub fn label(self) -> &'static str {
        match self {
            Speed::Fast => "Fast (larger file)",
            Speed::Balanced => "Balanced",
            Speed::Quality => "Best quality (slow)",
        }
    }
    fn pick<T: Copy>(self, fast: T, bal: T, qual: T) -> T {
        match self {
            Speed::Fast => fast,
            Speed::Balanced => bal,
            Speed::Quality => qual,
        }
    }
}

impl VideoCodec {
    pub fn label(self) -> &'static str {
        match self {
            VideoCodec::H264 => "H.264 (x264)",
            VideoCodec::H265 => "H.265 / HEVC (x265)",
            VideoCodec::Vp9 => "VP9",
            VideoCodec::Av1 => "AV1 (SVT-AV1)",
            VideoCodec::ProRes => "Apple ProRes 422 HQ",
            VideoCodec::H264Nvenc => "H.264 (NVIDIA NVENC)",
            VideoCodec::HevcNvenc => "H.265 (NVIDIA NVENC)",
            VideoCodec::H264Amf => "H.264 (AMD AMF)",
            VideoCodec::HevcAmf => "H.265 (AMD AMF)",
            VideoCodec::H264Qsv => "H.264 (Intel QSV)",
            VideoCodec::HevcQsv => "H.265 (Intel QSV)",
        }
    }
    pub fn supports_crf(self) -> bool {
        matches!(self, VideoCodec::H264 | VideoCodec::H265 | VideoCodec::Vp9 | VideoCodec::Av1)
    }
    pub fn uses_bitrate(self) -> bool {
        self != VideoCodec::ProRes
    }
    fn is_hevc(self) -> bool {
        matches!(self, VideoCodec::H265 | VideoCodec::HevcNvenc | VideoCodec::HevcAmf | VideoCodec::HevcQsv)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum AudioCodec {
    Aac,
    Mp3,
    Opus,
    Vorbis,
    Flac,
    Pcm,
}

impl AudioCodec {
    pub const ALL: [AudioCodec; 6] = [AudioCodec::Aac, AudioCodec::Mp3, AudioCodec::Opus, AudioCodec::Vorbis, AudioCodec::Flac, AudioCodec::Pcm];
    pub fn label(self) -> &'static str {
        match self {
            AudioCodec::Aac => "AAC",
            AudioCodec::Mp3 => "MP3",
            AudioCodec::Opus => "Opus",
            AudioCodec::Vorbis => "Vorbis",
            AudioCodec::Flac => "FLAC (lossless)",
            AudioCodec::Pcm => "PCM 16-bit (uncompressed)",
        }
    }
    pub fn uses_bitrate(self) -> bool {
        !matches!(self, AudioCodec::Flac | AudioCodec::Pcm)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum RateControl {
    Bitrate,
    Crf,
}

pub const RESOLUTIONS: [(&str, u32); 5] = [
    ("480p", 480),
    ("720p (HD)", 720),
    ("1080p (Full HD)", 1080),
    ("1440p (2K)", 1440),
    ("2160p (4K UHD)", 2160),
];

pub fn suggested_bitrate_kbps(short_side: u32) -> u32 {
    match short_side {
        0..=480 => 2_500,
        481..=720 => 5_000,
        721..=1080 => 10_000,
        1081..=1440 => 20_000,
        _ => 40_000,
    }
}

#[derive(Clone)]
pub struct RenderSettings {
    pub output: Option<PathBuf>,
    pub container: Container,
    pub short_side: u32,
    pub fps: u32,
    pub vcodec: VideoCodec,
    pub rate_control: RateControl,
    pub video_kbps: u32,
    pub crf: u32,
    pub acodec: AudioCodec,
    pub audio_kbps: u32,
    pub speed: Speed,
    /// Normalise the final mix to `lufs` (loudnorm).
    pub loudnorm: bool,
    pub lufs: f32,
    /// Burn the project's subtitles into the picture.
    pub burn_subs: bool,
    /// Also write an .srt file next to the export.
    pub sidecar_srt: bool,
    /// Export only between the in and out points (when both are set).
    pub use_range: bool,
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            output: None,
            container: Container::Mp4,
            short_side: 1080,
            fps: 30,
            vcodec: VideoCodec::H264,
            rate_control: RateControl::Bitrate,
            video_kbps: 10_000,
            crf: 20,
            acodec: AudioCodec::Aac,
            audio_kbps: 192,
            speed: Speed::Balanced,
            loudnorm: false,
            lufs: -14.0,
            burn_subs: true,
            sidecar_srt: false,
            use_range: true,
        }
    }
}

impl RenderSettings {
    /// Keep codec choices valid for the selected container.
    pub fn normalize(&mut self) {
        if !self.container.video_codecs().contains(&self.vcodec) {
            self.vcodec = self.container.video_codecs()[0];
        }
        if !self.container.audio_codecs().contains(&self.acodec) {
            self.acodec = self.container.audio_codecs()[0];
        }
        if !self.vcodec.supports_crf() {
            self.rate_control = RateControl::Bitrate;
        }
    }
}

/// A named set of export options (everything except the output file).
#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct ExportPreset {
    pub name: String,
    pub container: Container,
    pub short_side: u32,
    pub fps: u32,
    pub vcodec: VideoCodec,
    pub rate_control: RateControl,
    pub video_kbps: u32,
    pub crf: u32,
    pub acodec: AudioCodec,
    pub audio_kbps: u32,
    pub speed: Speed,
    pub loudnorm: bool,
    pub lufs: f32,
}

impl ExportPreset {
    pub fn from_settings(name: &str, s: &RenderSettings) -> ExportPreset {
        ExportPreset {
            name: name.to_string(),
            container: s.container,
            short_side: s.short_side,
            fps: s.fps,
            vcodec: s.vcodec,
            rate_control: s.rate_control,
            video_kbps: s.video_kbps,
            crf: s.crf,
            acodec: s.acodec,
            audio_kbps: s.audio_kbps,
            speed: s.speed,
            loudnorm: s.loudnorm,
            lufs: s.lufs,
        }
    }

    /// Copy these options into `s`, leaving the output file, subtitles and range options alone.
    pub fn apply(&self, s: &mut RenderSettings) {
        s.container = self.container;
        s.short_side = self.short_side;
        s.fps = self.fps;
        s.vcodec = self.vcodec;
        s.rate_control = self.rate_control;
        s.video_kbps = self.video_kbps;
        s.crf = self.crf;
        s.acodec = self.acodec;
        s.audio_kbps = self.audio_kbps;
        s.speed = self.speed;
        s.loudnorm = self.loudnorm;
        s.lufs = self.lufs;
        s.normalize();
    }

    /// Ready-made presets for common destinations.
    pub fn builtin() -> Vec<ExportPreset> {
        let p = |name: &str, container, short_side, fps, vcodec, video_kbps, acodec, audio_kbps, lufs: Option<f32>| ExportPreset {
            name: name.to_string(),
            container,
            short_side,
            fps,
            vcodec,
            rate_control: RateControl::Bitrate,
            video_kbps,
            crf: 20,
            acodec,
            audio_kbps,
            speed: Speed::Balanced,
            loudnorm: lufs.is_some(),
            lufs: lufs.unwrap_or(-14.0),
        };
        use AudioCodec as A;
        use Container as C;
        use VideoCodec as V;
        vec![
            p("YouTube 1080p", C::Mp4, 1080, 30, V::H264, 12_000, A::Aac, 192, Some(-14.0)),
            p("YouTube 4K", C::Mp4, 2160, 30, V::H265, 35_000, A::Aac, 256, Some(-14.0)),
            p("Instagram / TikTok (set canvas 9:16)", C::Mp4, 1080, 30, V::H264, 8_000, A::Aac, 192, Some(-14.0)),
            p("X / Twitter 720p", C::Mp4, 720, 30, V::H264, 5_000, A::Aac, 128, Some(-14.0)),
            p("Vimeo 1080p", C::Mp4, 1080, 30, V::H264, 20_000, A::Aac, 256, None),
            p("Small file (email / Discord)", C::Mp4, 720, 30, V::H264, 1_800, A::Aac, 96, None),
            p("Web (WebM VP9)", C::WebM, 1080, 30, V::Vp9, 5_000, A::Opus, 128, Some(-14.0)),
            p("Broadcast 1080p25 (-23 LUFS)", C::Mov, 1080, 25, V::H264, 25_000, A::Pcm, 1536, Some(-23.0)),
            p("Archive (ProRes)", C::Mov, 1080, 30, V::ProRes, 0, A::Pcm, 1536, None),
        ]
    }
}

pub enum RenderMsg {
    Progress(f32),
    Done,
    Cancelled,
    Failed(String),
}

fn f(v: f64) -> String {
    format!("{:.3}", v)
}

fn esc_path(p: &std::path::Path) -> String {
    p.to_string_lossy().replace('\\', "/").replace(':', "\\:")
}

/// Piecewise-linear FFmpeg expression of keyframes `kfs` in variable expression `var`.
fn kf_expr(kfs: &[Keyframe], var: &str, default: f64) -> String {
    match kfs.len() {
        0 => format!("{default:.4}"),
        1 => format!("{:.4}", kfs[0].v),
        _ => {
            let mut e = format!("{:.4}", kfs[kfs.len() - 1].v);
            for w in kfs.windows(2).rev() {
                let span = (w[1].t - w[0].t).max(1e-6);
                let p = format!("(({var}-{:.4})/{:.4})", w[0].t, span);
                e = format!("if(lt({var},{:.4}),{:.4}+({:.4})*{},{e})", w[1].t, w[0].v, w[1].v - w[0].v, w[0].ease.expr(&p));
            }
            format!("if(lt({var},{:.4}),{:.4},{e})", kfs[0].t, kfs[0].v)
        }
    }
}

fn ramp(num: &str, d: f64) -> String {
    format!("min(1,max(0,({num})/{:.4}))", d.max(0.01))
}

fn hex(c: [u8; 3]) -> String {
    format!("0x{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

fn effect_filter(e: &Effect, uid: &str) -> String {
    match e {
        Effect::ColorAdjust { brightness, contrast, saturation } => {
            format!("eq=brightness={brightness:.3}:contrast={contrast:.3}:saturation={saturation:.3}")
        }
        Effect::Blur { sigma } => format!("gblur=sigma={:.2}", sigma.max(0.1)),
        Effect::Sharpen { amount } => format!("unsharp=5:5:{amount:.2}"),
        Effect::Grayscale => "hue=s=0".to_string(),
        Effect::Sepia => "colorchannelmixer=.393:.769:.189:0:.349:.686:.168:0:.272:.534:.131:0".to_string(),
        Effect::Invert => "negate".to_string(),
        Effect::Vignette => "vignette=PI/4".to_string(),
        Effect::FlipH => "hflip".to_string(),
        Effect::FlipV => "vflip".to_string(),
        Effect::ChromaKey { color, similarity, blend } => {
            format!("chromakey=color={}:similarity={similarity:.3}:blend={blend:.3}", hex(*color))
        }
        // Zoom is applied through the layer's dynamic scale and overlay position, not a filter.
        Effect::Zoom { .. } => String::new(),
        Effect::ColorWheels { lift, gamma, gain } => {
            if lift.iter().chain(gamma).chain(gain).all(|v| v.abs() < 1e-4) {
                return String::new();
            }
            format!(
                "colorbalance=rs={:.3}:gs={:.3}:bs={:.3}:rm={:.3}:gm={:.3}:bm={:.3}:rh={:.3}:gh={:.3}:bh={:.3}",
                lift[0], lift[1], lift[2], gamma[0], gamma[1], gamma[2], gain[0], gain[1], gain[2]
            )
        }
        Effect::Temperature { kelvin, tint } => {
            let mut parts: Vec<String> = vec![];
            if (kelvin - 6500.0).abs() > 1.0 {
                parts.push(format!("colortemperature=temperature={:.0}", kelvin.clamp(1000.0, 40000.0)));
            }
            if tint.abs() > 1e-3 {
                parts.push(format!("colorbalance=gm={:.3}", (-tint * 0.4).clamp(-1.0, 1.0)));
            }
            parts.join(",")
        }
        Effect::Lut { path } => {
            if path.is_empty() {
                String::new()
            } else {
                format!("lut3d=file='{}'", esc_path(std::path::Path::new(path)))
            }
        }
        Effect::HslAdjust { band, hue, sat, intensity } => {
            format!("huesaturation=hue={hue:.2}:saturation={sat:.3}:intensity={intensity:.3}:colors={}", band.ff())
        }
        Effect::Curves { preset } => format!("curves=preset={}", preset.ff()),
        Effect::Glow { radius, strength } => {
            format!(
                "split[gla{uid}][glb{uid}];[gla{uid}]gblur=sigma={:.1}[glc{uid}];[glb{uid}][glc{uid}]blend=all_mode=screen:all_opacity={:.3}",
                radius.max(1.0),
                strength.clamp(0.0, 1.0)
            )
        }
        Effect::Distort { amount } => format!("lenscorrection=k1={a:.3}:k2={a:.3}", a = amount.clamp(-0.9, 0.9)),
        Effect::Rotate { degrees } => format!("format=yuva420p,rotate=a={:.5}:ow=iw:oh=ih:c=black@0", degrees.to_radians()),
        Effect::Pixelate { block } => {
            let b = (block.round() as i32).clamp(2, 256);
            format!("pixelize=w={b}:h={b}")
        }
        Effect::Grain { amount } => format!("noise=alls={}:allf=t+u", (amount.round() as i32).clamp(1, 100)),
        Effect::Denoise { strength } => format!("hqdn3d={:.1}", strength.clamp(0.5, 20.0)),
        Effect::Stabilize { strength } => {
            let r = (strength.round() as i32).clamp(1, 64);
            format!("deshake=rx={r}:ry={r}:edge=mirror")
        }
        Effect::Mask { ellipse, cx, cy, w, h, feather, invert } => {
            let (rx, ry) = ((w / 2.0).max(0.01), (h / 2.0).max(0.01));
            let d = if *ellipse {
                format!("hypot((X/W-{cx:.4})/{rx:.4},(Y/H-{cy:.4})/{ry:.4})")
            } else {
                format!("max(abs(X/W-{cx:.4})/{rx:.4},abs(Y/H-{cy:.4})/{ry:.4})")
            };
            let f = (feather * 2.0).max(0.002);
            let inside = format!("min(1,max(0,(1-{d})/{f:.4}))");
            let m = if *invert { format!("(1-{inside})") } else { inside };
            format!("format=yuva420p,geq=lum='lum(X,Y)':cb='cb(X,Y)':cr='cr(X,Y)':a='alpha(X,Y)*{m}'")
        }
    }
}

/// Filters applied to one video layer (after its source), plus overlay x / y expressions.
fn layer_chain(c: &Clip, rw: u32, rh: u32, fps: u32, fit: bool) -> (Vec<String>, String, String) {
    let fit_scale = format!("scale={rw}:{rh}:force_original_aspect_ratio=decrease:flags=lanczos");
    let mut v: Vec<String>;
    if c.freeze {
        // One source frame, held for the whole clip.
        v = vec!["trim=end_frame=1".to_string(), "setpts=PTS-STARTPTS".to_string()];
        if fit {
            v.push(fit_scale);
            v.push("setsar=1".to_string());
        }
        v.push(format!("tpad=stop_mode=clone:stop_duration={:.3}", c.duration + 1.0));
        v.push(format!("fps={fps}"));
        v.push(format!("trim=duration={:.3}", c.duration));
        v.push("setpts=PTS-STARTPTS".to_string());
    } else if c.reverse || (c.speed - 1.0).abs() > 1e-6 {
        // Fit first so `reverse` buffers small frames, then retime.
        v = vec!["setpts=PTS-STARTPTS".to_string()];
        if fit {
            v.push(fit_scale);
            v.push("setsar=1".to_string());
        }
        if c.reverse {
            v.push("reverse".to_string());
        }
        v.push(format!("setpts=PTS/{:.6}", c.speed.max(0.01)));
        if c.interpolate && c.speed < 0.999 {
            v.push(format!("minterpolate=fps={fps}:mi_mode=mci:mc_mode=aobmc:vsbmc=1"));
        }
        v.push(format!("fps={fps}"));
    } else {
        v = vec!["setpts=PTS-STARTPTS".to_string(), format!("fps={fps}")];
        if fit {
            v.push(fit_scale);
            v.push("setsar=1".to_string());
        }
    }
    for (k, e) in c.effects.iter().enumerate() {
        let ef = effect_filter(e, &format!("{}_{k}", c.id));
        if !ef.is_empty() {
            v.push(ef);
        }
    }

    let off = c.anim_offset;
    // A playback window that starts inside the clip has already passed its fade-in.
    let tin = if off > 1e-6 { Transition::default() } else { c.transition_in };
    let tout = c.transition_out;
    let uses_fade = |t: &Transition| matches!(t.kind, TransitionKind::Fade | TransitionKind::Zoom);
    let keyed_alpha = !c.anim.opacity.is_empty();
    let chroma = c.effects.iter().any(|e| matches!(e, Effect::ChromaKey { .. } | Effect::Mask { .. } | Effect::Rotate { .. }));
    if uses_fade(&tin) || uses_fade(&tout) || keyed_alpha || chroma {
        v.push("format=yuva420p".to_string());
    }

    if keyed_alpha {
        let e = kf_expr(&c.anim.opacity, &format!("(T+{off:.4})"), 1.0);
        v.push(format!("geq=lum='lum(X,Y)':cb='cb(X,Y)':cr='cr(X,Y)':a='alpha(X,Y)*min(1,max(0,{e}))'"));
    }
    if uses_fade(&tin) && tin.duration > 0.0 {
        v.push(format!("fade=t=in:st=0:d={:.3}:alpha=1", tin.duration));
    }
    if uses_fade(&tout) && tout.duration > 0.0 {
        v.push(format!("fade=t=out:st={:.3}:d={:.3}:alpha=1", (c.duration - tout.duration).max(0.0), tout.duration));
    }
    // Dynamic scale goes last: it changes the frame size per frame, which would corrupt any
    // fixed-size filter after it (geq / fade segfault in FFmpeg). Only overlay tolerates it.
    let mut terms: Vec<String> = vec![];
    if !c.anim.scale.is_empty() {
        terms.push(format!("({})", kf_expr(&c.anim.scale, &format!("(t+{off:.4})"), 1.0)));
    }
    // Zoom effect (first one wins): factor over the whole clip, optionally eased.
    let zoom = c.effects.iter().find_map(|e| match e {
        Effect::Zoom { from, to, fx, fy, ease } => Some((*from as f64, *to as f64, *fx as f64, *fy as f64, *ease)),
        _ => None,
    });
    let zoom_expr = |var: &str| -> Option<String> {
        let (from, to, _, _, ease) = zoom?;
        let total = (c.duration + off).max(0.05);
        let p = format!("min(1,max(0,({var})/{total:.4}))");
        let p = if ease { format!("({p}*{p}*(3-2*{p}))") } else { format!("({p})") };
        Some(format!("({from:.4}+({to:.4}-{from:.4})*{p})"))
    };
    if let Some(z) = zoom_expr(&format!("t+{off:.4}")) {
        terms.push(z);
    }
    if tin.kind == TransitionKind::Zoom {
        terms.push(format!("(0.6+0.4*{})", ramp("t", tin.duration)));
    }
    if tout.kind == TransitionKind::Zoom {
        terms.push(format!("(0.6+0.4*{})", ramp(&format!("{:.4}-t", c.duration), tout.duration)));
    }
    if !terms.is_empty() {
        let t = terms.join("*");
        v.push(format!("scale=w='max(2,trunc(iw*({t})/2)*2)':h='max(2,trunc(ih*({t})/2)*2)':eval=frame"));
    }

    v.push(format!("setpts=PTS+{}/TB", f(c.start)));

    // overlay position (timeline time `t`)
    let rel = format!("(t-{:.4}+{off:.4})", c.start);
    let mut x = "(W-w)/2".to_string();
    let mut y = "(H-h)/2".to_string();
    if !c.anim.pos_x.is_empty() {
        x = format!("{x}+W*({})", kf_expr(&c.anim.pos_x, &rel, 0.0));
    }
    if !c.anim.pos_y.is_empty() {
        y = format!("{y}+H*({})", kf_expr(&c.anim.pos_y, &rel, 0.0));
    }
    if let (Some((_, _, fx, fy, _)), Some(z)) = (zoom, zoom_expr(&format!("t-{:.4}+{off:.4}", c.start))) {
        // Keep the focus point of the picture fixed while it scales about its centre.
        if (fx - 0.5).abs() > 1e-4 {
            x = format!("{x}+(0.5-{fx:.4})*w*(1-1/{z})");
        }
        if (fy - 0.5).abs() > 1e-4 {
            y = format!("{y}+(0.5-{fy:.4})*h*(1-1/{z})");
        }
    }
    let p_in = |d: f64| ramp(&format!("t-{:.4}", c.start), d);
    let p_out = |d: f64| ramp(&format!("{:.4}-t", c.end()), d);
    for (tr, is_in) in [(&tin, true), (&tout, false)] {
        if tr.duration <= 0.0 {
            continue;
        }
        let p = if is_in { p_in(tr.duration) } else { p_out(tr.duration) };
        // Entering slides travel from the far side; exiting slides leave toward the near side.
        let sign = if is_in { "+" } else { "-" };
        match tr.kind {
            TransitionKind::SlideLeft => x = format!("{x}{sign}W*(1-{p})"),
            TransitionKind::SlideRight => x = format!("{x}{}W*(1-{p})", if is_in { "-" } else { "+" }),
            TransitionKind::SlideUp => y = format!("{y}{sign}H*(1-{p})"),
            TransitionKind::SlideDown => y = format!("{y}{}H*(1-{p})", if is_in { "-" } else { "+" }),
            _ => {}
        }
    }
    (v, x, y)
}

/// Greedy word wrap using an average glyph width. FFmpeg's drawtext does no wrapping itself.
fn wrap_text(text: &str, max_chars: usize) -> String {
    let max_chars = max_chars.max(1);
    let mut out: Vec<String> = vec![];
    for para in text.lines() {
        let mut line = String::new();
        for word in para.split_whitespace() {
            let wl = word.chars().count();
            if line.is_empty() {
                line.push_str(word);
            } else if line.chars().count() + 1 + wl <= max_chars {
                line.push(' ');
                line.push_str(word);
            } else {
                out.push(std::mem::take(&mut line));
                line.push_str(word);
            }
        }
        out.push(line);
    }
    out.join("\n")
}

/// Estimated pixel size of a title's text block (used for auto-fit boxes and wrapping).
pub fn title_text_extent(text: &str, size_px: f32) -> (f32, f32) {
    let lines = text.lines().count().max(1) as f32;
    let widest = text.lines().map(|l| l.chars().count()).max().unwrap_or(1).max(1) as f32;
    (widest * size_px * 0.55, lines * size_px * 1.25)
}

/// Source filter that renders a title card on a transparent canvas.
fn title_source(c: &Clip, spec: &TitleSpec, rw: u32, rh: u32, fps: u32) -> String {
    let file = std::env::temp_dir().join(format!("reelforge_title_{}.txt", c.id));
    let size = (spec.size.max(1.0) / 100.0 * rh as f32).round().max(8.0);
    let pad = (size * 0.3).round();
    let custom_box = spec.box_w > 0.0 || spec.box_h > 0.0;
    let font = format!("C\\:/Windows/Fonts/{}", spec.font.file());
    let bg = format!("{}@{:.3}", hex(spec.bg_color), spec.bg_opacity.clamp(0.0, 1.0));
    let head = format!("color=c=black@0.0:s={rw}x{rh}:r={fps}:d={:.3},format=rgba", c.duration);

    if !custom_box {
        let _ = std::fs::write(&file, spec.text.as_bytes());
        let (xpos, ypos) = if let Some([px, py]) = spec.pos {
            (format!("w*{px:.4}-text_w/2"), format!("h*{py:.4}-text_h/2"))
        } else {
            let y = match spec.position {
                TitlePos::Center => "(h-text_h)/2".to_string(),
                TitlePos::LowerThird => "h*0.76-text_h/2".to_string(),
                TitlePos::Top => "h*0.08".to_string(),
            };
            ("(w-text_w)/2".to_string(), y)
        };
        let mut dt = format!(
            "drawtext=fontfile='{font}':textfile='{}':fontsize={size}:fontcolor={}:x='{xpos}':y='{ypos}':line_spacing=6",
            esc_path(&file),
            hex(spec.color)
        );
        if spec.background {
            dt.push_str(&format!(":box=1:boxcolor={bg}:boxborderw={pad}"));
        }
        return format!("{head},{dt},format=rgba");
    }

    // Fixed-size box: wrap the text to the box width, centre it, and paint a solid rectangle.
    let inner_w = if spec.box_w > 0.0 { (spec.box_w * rw as f32 - 2.0 * pad).max(size) } else { f32::MAX };
    let text = if spec.box_w > 0.0 { wrap_text(&spec.text, (inner_w / (size * 0.52)).floor() as usize) } else { spec.text.clone() };
    let _ = std::fs::write(&file, text.as_bytes());
    let (tw, th) = title_text_extent(&text, size);
    let bw = if spec.box_w > 0.0 { spec.box_w * rw as f32 } else { tw + 2.0 * pad };
    let bh = if spec.box_h > 0.0 { spec.box_h * rh as f32 } else { th + 2.0 * pad };
    let (cx, cy) = match spec.pos {
        Some([x, y]) => (x * rw as f32, y * rh as f32),
        None => {
            let y = match spec.position {
                TitlePos::Center => 0.5 * rh as f32,
                TitlePos::LowerThird => 0.76 * rh as f32,
                TitlePos::Top => 0.08 * rh as f32 + bh / 2.0,
            };
            (0.5 * rw as f32, y)
        }
    };
    let mut parts = vec![head];
    if spec.background {
        parts.push(format!(
            "drawbox=x={}:y={}:w={}:h={}:color={bg}:t=fill:replace=1",
            (cx - bw / 2.0).round(),
            (cy - bh / 2.0).round(),
            bw.round(),
            bh.round()
        ));
    }
    parts.push(format!(
        "drawtext=fontfile='{font}':textfile='{}':fontsize={size}:fontcolor={}:x='{cx:.1}-text_w/2':y='{cy:.1}-text_h/2':line_spacing=6:text_align=C",
        esc_path(&file),
        hex(spec.color)
    ));
    parts.push("format=rgba".to_string());
    parts.join(",")
}

/// `atempo` only accepts 0.5-2.0 per stage, so chain stages for other speeds (empty at 1x).
fn atempo_chain(speed: f64) -> String {
    let mut s = speed.clamp(0.1, 16.0);
    if (s - 1.0).abs() < 1e-6 {
        return String::new();
    }
    let mut out = String::new();
    while s > 2.0 {
        out.push_str("atempo=2.0,");
        s /= 2.0;
    }
    while s < 0.5 {
        out.push_str("atempo=0.5,");
        s /= 0.5;
    }
    out.push_str(&format!("atempo={s:.6},"));
    out
}

/// Build FFmpeg input arguments and the filter graph. Produces label [vout] when `video`
/// and [aout] when `audio`. Returns (input args, filter graph, total duration).
pub fn build_graph(project: &Project, rw: u32, rh: u32, fps: u32, video: bool, audio: bool) -> (Vec<String>, String, f64) {
    build_graph_ex(project, rw, rh, fps, video, audio, &GraphOpts { loudnorm: None, subs: true })
}

#[derive(Clone, Copy, Debug)]
pub struct GraphOpts {
    /// Normalise the final mix to this LUFS target.
    pub loudnorm: Option<f32>,
    /// Draw the project's subtitles.
    pub subs: bool,
}

/// Composite a finished layer `lab` over `current`, honouring the clip's blend mode.
/// Returns the label of the new base.
#[allow(clippy::too_many_arguments)]
fn composite(filters: &mut Vec<String>, current: &str, lab: &str, x: &str, y: &str, c: &Clip, n: usize, rw: u32, rh: u32, fps: u32, total: f64) -> String {
    let next = format!("l{n}");
    let gate = format!("enable='between(t,{},{})'", f(c.start), f(c.end()));
    if c.blend == BlendMode::Normal {
        filters.push(format!("[{current}][{lab}]overlay=x='{x}':y='{y}':eof_action=pass:{gate}[{next}]"));
    } else {
        // Blend modes need a full-frame layer: place the layer on a transparent canvas, blend that
        // with the picture below, then use the layer's alpha as a mask so only its area changes.
        filters.push(format!("color=c=black@0.0:s={rw}x{rh}:r={fps}:d={},format=rgba[cv{n}]", f(total)));
        filters.push(format!("[cv{n}][{lab}]overlay=x='{x}':y='{y}':eof_action=pass:format=auto:{gate},split[lfa{n}][lfb{n}]"));
        filters.push(format!("[lfa{n}]alphaextract[mk{n}]"));
        filters.push(format!("[lfb{n}]format=yuv420p[lfc{n}]"));
        filters.push(format!("[{current}]split[cs{n}a][cs{n}b]"));
        filters.push(format!("[cs{n}a][lfc{n}]blend=all_mode={}[bl{n}]", c.blend.ff()));
        filters.push(format!("[bl{n}][mk{n}]alphamerge[bm{n}]"));
        filters.push(format!("[cs{n}b][bm{n}]overlay=eof_action=pass[{next}]"));
    }
    next
}

/// FFmpeg audio filters for a clip's pan / EQ / dynamics / space effects (trailing comma).
fn audio_fx_chain(fx: &AudioFx) -> String {
    let mut v: Vec<String> = vec![];
    if fx.denoise > 0.01 {
        v.push(format!("afftdn=nr={:.1}:nf=-45", 6.0 + fx.denoise.clamp(0.0, 1.0) * 30.0));
    }
    if fx.deess > 0.01 {
        v.push(format!("deesser=i={:.2}:m=0.5:f=0.5", fx.deess.clamp(0.0, 1.0)));
    }
    if fx.eq_low.abs() > 0.05 {
        v.push(format!("bass=g={:.1}:f=120", fx.eq_low));
    }
    if fx.eq_mid.abs() > 0.05 {
        v.push(format!("equalizer=f=1000:t=q:w=0.8:g={:.1}", fx.eq_mid));
    }
    if fx.eq_high.abs() > 0.05 {
        v.push(format!("treble=g={:.1}:f=6000", fx.eq_high));
    }
    if fx.compress > 0.01 {
        let a = fx.compress.clamp(0.0, 1.0) as f64;
        let thr_db = -12.0 - 24.0 * a;
        v.push(format!("acompressor=threshold={:.5}:ratio={:.1}:attack=10:release=200:makeup={:.2}", 10f64.powf(thr_db / 20.0), 2.0 + 6.0 * a, 1.0 + 3.0 * a));
    }
    if fx.reverb > 0.01 {
        let r = fx.reverb.clamp(0.0, 1.0);
        v.push(format!("aecho=0.8:0.85:40|75|120|180:{:.2}|{:.2}|{:.2}|{:.2}", 0.5 * r, 0.4 * r, 0.3 * r, 0.2 * r));
    }
    if fx.echo_ms > 1.0 {
        v.push(format!("aecho=0.8:0.88:{:.0}:{:.2}", fx.echo_ms.clamp(1.0, 2000.0), fx.echo_decay.clamp(0.0, 0.9)));
    }
    if fx.normalize {
        v.push("loudnorm=I=-16:TP=-1.5:LRA=11".to_string());
    }
    if fx.pan.abs() > 0.005 {
        let l = 1.0 - fx.pan.max(0.0);
        let r = 1.0 + fx.pan.min(0.0);
        v.push(format!("pan=stereo|c0={l:.3}*c0|c1={r:.3}*c1"));
    }
    if v.is_empty() {
        String::new()
    } else {
        format!("{},", v.join(","))
    }
}

pub fn build_graph_ex(project: &Project, rw: u32, rh: u32, fps: u32, video: bool, audio: bool, opts: &GraphOpts) -> (Vec<String>, String, f64) {
    let total = project.duration();
    let mut args: Vec<String> = vec![];

    // Order clips bottom layer first so upper tracks overlay lower ones.
    let mut clips: Vec<_> = project.clips.iter().collect();
    clips.sort_by(|a, b| a.track.cmp(&b.track).then(a.start.partial_cmp(&b.start).unwrap()));

    let mut filters: Vec<String> = vec![];
    let mut audio_labels: Vec<String> = vec![];
    let mut current = "base".to_string();
    if video {
        filters.push(format!("color=c=black:s={rw}x{rh}:r={fps}:d={}[base]", f(total)));
    }

    let mut input_idx = 0usize;
    let mut layer_no = 0usize;
    for c in clips {
        let Some(track) = project.tracks.get(c.track) else { continue };

        // ---- generated title clip: no input file ----
        if let Some(spec) = &c.title {
            if !video || track.hidden {
                continue;
            }
            let (chain, x, y) = layer_chain(c, rw, rh, fps, false);
            layer_no += 1;
            let lab = format!("t{layer_no}");
            filters.push(format!("{},{}[{lab}]", title_source(c, spec, rw, rh, fps), chain.join(",")));
            current = composite(&mut filters, &current, &lab, &x, &y, c, layer_no, rw, rh, fps, total);
            continue;
        }

        let Some(m) = project.media(c.media_id) else { continue };
        let wants_v = video && m.kind != MediaKind::Audio && !track.hidden;
        let wants_a = audio && m.has_audio && m.kind != MediaKind::Image && !track.muted && !c.freeze;
        if !wants_v && !wants_a {
            continue;
        }
        let path = m.path.to_string_lossy().into_owned();

        match m.kind {
            MediaKind::Image => {
                args.extend(["-loop".to_string(), "1".to_string(), "-framerate".to_string(), fps.to_string(), "-t".to_string(), f(c.duration), "-i".to_string(), path]);
            }
            _ if c.freeze => {
                args.extend(["-ss".to_string(), f(c.in_point), "-t".to_string(), "1".to_string(), "-i".to_string(), path]);
            }
            _ => {
                args.extend(["-ss".to_string(), f(c.in_point), "-t".to_string(), f(c.src_span()), "-i".to_string(), path]);
            }
        }
        let i = input_idx;
        input_idx += 1;

        if wants_v {
            let (chain, x, y) = layer_chain(c, rw, rh, fps, true);
            filters.push(format!("[{i}:v]{}[v{i}]", chain.join(",")));
            layer_no += 1;
            current = composite(&mut filters, &current, &format!("v{i}"), &x, &y, c, layer_no, rw, rh, fps, total);
        }

        if wants_a {
            let ms = (c.start * 1000.0).round() as i64;
            let mut fades = String::new();
            // A window that starts inside the clip is already past its fade-in.
            if c.audio_fade_in > 0.0 && c.anim_offset <= 1e-6 {
                fades.push_str(&format!("afade=t=in:st=0:d={:.3},", c.audio_fade_in));
            }
            if c.audio_fade_out > 0.0 {
                fades.push_str(&format!("afade=t=out:st={:.3}:d={:.3},", (c.duration - c.audio_fade_out).max(0.0), c.audio_fade_out));
            }
            let mut retime = String::new();
            if c.reverse {
                retime.push_str("areverse,");
            }
            retime.push_str(&atempo_chain(c.speed));
            retime.push_str(&audio_fx_chain(&c.afx));
            if !c.anim.volume.is_empty() {
                let e = kf_expr(&c.anim.volume, &format!("(t+{:.4})", c.anim_offset), 1.0);
                retime.push_str(&format!("volume=volume='{e}':eval=frame,"));
            }
            filters.push(format!(
                "[{i}:a]aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo,asetpts=PTS-STARTPTS,{retime}volume={:.3},{fades}adelay={ms}|{ms}[a{i}]",
                c.volume
            ));
            audio_labels.push(format!("[a{i}]"));
        }
    }

    if video {
        let mut tail = String::new();
        if opts.subs && !project.subtitles.is_empty() {
            let srt = std::env::temp_dir().join("reelforge_subs.srt");
            if std::fs::write(&srt, interchange::to_srt(&project.subtitles)).is_ok() {
                let dir = if cfg!(windows) { ":fontsdir='C\\:/Windows/Fonts'" } else { "" };
                tail = format!(
                    "subtitles=filename='{}'{dir}:force_style='FontName=Segoe UI,Fontsize=18,PrimaryColour=&HFFFFFF&,OutlineColour=&H80000000&,BorderStyle=3,Outline=2,Shadow=0,MarginV=18',",
                    esc_path(&srt)
                );
            }
        }
        filters.push(format!("[{current}]{tail}format=yuv420p[vout]"));
    }
    if audio {
        // A silent bed guarantees an audio stream of the full length, even with no audio clips.
        filters.push(format!("anullsrc=r=48000:cl=stereo,atrim=duration={}[asil]", f(total)));
        let n = audio_labels.len() + 1;
        filters.push(format!(
            "[asil]{}amix=inputs={n}:duration=longest:normalize=0,alimiter=limit=0.95[amx]",
            audio_labels.join("")
        ));
        match opts.loudnorm {
            Some(l) => filters.push(format!("[amx]loudnorm=I={l:.1}:TP=-1.5:LRA=11[aout]")),
            None => filters.push("[amx]anull[aout]".to_string()),
        }
    }
    (args, filters.join(";"), total)
}

/// Encoder options shared by export and the recorders: video codec / rate control / pixel
/// format, audio codec / bitrate, and MP4 flags. `faststart` is off for live recordings, which
/// use fragmented MP4 so a crash or power loss still leaves a playable file.
pub fn encoder_args(s: &RenderSettings, video: bool, audio: bool, faststart: bool) -> Vec<String> {
    let mut args: Vec<String> = vec![];
    let fps = s.fps;
    if video {
    // ---- video encoder ----
    let kbps = s.video_kbps.max(100);
    let br = |args: &mut Vec<String>| {
        args.extend([
            "-b:v".to_string(),
            format!("{kbps}k"),
            "-maxrate".to_string(),
            format!("{}k", kbps * 3 / 2),
            "-bufsize".to_string(),
            format!("{}k", kbps * 2),
        ]);
    };
    let sp = s.speed;
    let use_crf = s.rate_control == RateControl::Crf && s.vcodec.supports_crf();
    match s.vcodec {
        VideoCodec::H264 | VideoCodec::H265 => {
            let enc = if s.vcodec == VideoCodec::H264 { "libx264" } else { "libx265" };
            args.extend(["-c:v".to_string(), enc.to_string(), "-preset".to_string(), sp.pick("veryfast", "medium", "slow").to_string()]);
            if use_crf {
                args.extend(["-crf".to_string(), s.crf.to_string()]);
            } else {
                br(&mut args);
            }
            if s.vcodec == VideoCodec::H265 {
                args.extend(["-x265-params".to_string(), "log-level=error".to_string()]);
            }
        }
        VideoCodec::Vp9 => {
            args.extend([
                "-c:v".to_string(),
                "libvpx-vp9".to_string(),
                "-row-mt".to_string(),
                "1".to_string(),
                "-cpu-used".to_string(),
                sp.pick("5", "2", "1").to_string(),
            ]);
            if use_crf {
                args.extend(["-crf".to_string(), s.crf.to_string(), "-b:v".to_string(), "0".to_string()]);
            } else {
                args.extend(["-b:v".to_string(), format!("{kbps}k")]);
            }
        }
        VideoCodec::Av1 => {
            args.extend(["-c:v".to_string(), "libsvtav1".to_string(), "-preset".to_string(), sp.pick("10", "6", "4").to_string()]);
            if use_crf {
                args.extend(["-crf".to_string(), s.crf.to_string()]);
            } else {
                args.extend(["-b:v".to_string(), format!("{kbps}k")]);
            }
        }
        VideoCodec::ProRes => {
            args.extend(["-c:v".to_string(), "prores_ks".to_string(), "-profile:v".to_string(), "3".to_string()]);
        }
        VideoCodec::H264Nvenc | VideoCodec::HevcNvenc => {
            let enc = if s.vcodec == VideoCodec::H264Nvenc { "h264_nvenc" } else { "hevc_nvenc" };
            args.extend(["-c:v".to_string(), enc.to_string(), "-preset".to_string(), sp.pick("p3", "p5", "p7").to_string(), "-rc".to_string(), "vbr".to_string()]);
            br(&mut args);
        }
        VideoCodec::H264Amf | VideoCodec::HevcAmf => {
            let enc = if s.vcodec == VideoCodec::H264Amf { "h264_amf" } else { "hevc_amf" };
            args.extend(["-c:v".to_string(), enc.to_string(), "-quality".to_string(), sp.pick("speed", "balanced", "quality").to_string()]);
            br(&mut args);
        }
        VideoCodec::H264Qsv | VideoCodec::HevcQsv => {
            let enc = if s.vcodec == VideoCodec::H264Qsv { "h264_qsv" } else { "hevc_qsv" };
            args.extend(["-c:v".to_string(), enc.to_string(), "-preset".to_string(), sp.pick("veryfast", "medium", "veryslow").to_string()]);
            br(&mut args);
        }
    }
    let pix = if s.vcodec == VideoCodec::ProRes { "yuv422p10le" } else { "yuv420p" };
    args.extend(["-pix_fmt".to_string(), pix.to_string(), "-r".to_string(), fps.to_string()]);
    if s.vcodec.is_hevc() && matches!(s.container, Container::Mp4 | Container::Mov) {
        args.extend(["-tag:v".to_string(), "hvc1".to_string()]);
    }

    }
    if audio {
    // ---- audio encoder ----
    let a = match s.acodec {
        AudioCodec::Aac => "aac",
        AudioCodec::Mp3 => "libmp3lame",
        AudioCodec::Opus => "libopus",
        AudioCodec::Vorbis => "libvorbis",
        AudioCodec::Flac => "flac",
        AudioCodec::Pcm => "pcm_s16le",
    };
    args.extend(["-c:a".to_string(), a.to_string()]);
    if s.acodec.uses_bitrate() {
        args.extend(["-b:a".to_string(), format!("{}k", s.audio_kbps)]);
    }
    args.extend(["-ar".to_string(), "48000".to_string(), "-ac".to_string(), "2".to_string()]);

    }
    if video && matches!(s.container, Container::Mp4 | Container::Mov) {
        if faststart {
            args.extend(["-movflags".to_string(), "+faststart".to_string()]);
        } else {
            args.extend(["-movflags".to_string(), "+frag_keyframe+empty_moov+default_base_moof".to_string()]);
        }
    }
    args
}

/// Build the complete FFmpeg argument list for the project.
pub fn build_args(project: &Project, s: &RenderSettings, out: &PathBuf) -> Result<Vec<String>, String> {
    let total = project.duration();
    if total <= 0.0 {
        return Err("The timeline is empty.".to_string());
    }
    let (rw, rh) = project.output_size(s.short_side);
    let fps = s.fps;

    let mut args: Vec<String> = ["-y", "-hide_banner", "-v", "error", "-nostdin", "-progress", "pipe:1", "-nostats"]
        .iter()
        .map(|x| x.to_string())
        .collect();

    let opts = GraphOpts { loudnorm: if s.loudnorm { Some(s.lufs) } else { None }, subs: s.burn_subs };
    let (inputs, graph, _) = build_graph_ex(project, rw, rh, fps, true, true, &opts);
    args.extend(inputs);
    args.extend(["-filter_complex".to_string(), graph]);
    args.extend(["-map".to_string(), "[vout]".to_string(), "-map".to_string(), "[aout]".to_string()]);

    args.extend(encoder_args(s, true, true, true));
    args.extend(["-t".to_string(), f(total)]);
    args.push(out.to_string_lossy().into_owned());
    Ok(args)
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Loudness {
    /// Integrated loudness, LUFS.
    pub integrated: f64,
    /// Loudness range, LU.
    pub range: f64,
    /// True peak, dBTP.
    pub true_peak: f64,
}

/// Parse the summary block `ebur128` prints at the end of a run.
pub fn parse_ebur128(log: &str) -> Option<Loudness> {
    let tail = &log[log.rfind("Summary:")?..];
    let num = |key: &str| -> Option<f64> {
        let i = tail.find(key)? + key.len();
        tail[i..].trim_start().split_whitespace().next()?.parse().ok()
    };
    Some(Loudness { integrated: num("I:")?, range: num("LRA:")?, true_peak: num("Peak:")? })
}

/// Measure the loudness of the project's mixed audio (blocking; run on a thread).
pub fn measure_loudness(project: &Project) -> Result<Loudness, String> {
    if project.duration() <= 0.0 {
        return Err("The timeline is empty.".into());
    }
    let (inputs, graph, _) = build_graph(project, 16, 16, 10, false, true);
    let out = command("ffmpeg")
        .args(["-hide_banner", "-nostdin", "-nostats", "-v", "info"])
        .args(inputs)
        .args(["-filter_complex", &format!("{graph};[aout]ebur128=peak=true:framelog=quiet[lo]"), "-map", "[lo]", "-f", "null", "-"])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    let log = String::from_utf8_lossy(&out.stderr).into_owned();
    parse_ebur128(&log).ok_or_else(|| "Could not read the loudness result from FFmpeg.".to_string())
}

pub fn start(ctx: egui::Context, project: Project, settings: RenderSettings) -> (Receiver<RenderMsg>, Arc<AtomicBool>) {
    let (tx, rx) = channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_t = cancel.clone();

    std::thread::spawn(move || {
        let send = |m: RenderMsg| {
            let _ = tx.send(m);
            ctx.request_repaint();
        };
        let Some(out) = settings.output.clone() else {
            return send(RenderMsg::Failed("No output file chosen.".to_string()));
        };
        let args = match build_args(&project, &settings, &out) {
            Ok(a) => a,
            Err(e) => return send(RenderMsg::Failed(e)),
        };
        if settings.sidecar_srt && !project.subtitles.is_empty() {
            let _ = std::fs::write(out.with_extension("srt"), interchange::to_srt(&project.subtitles));
        }
        let total_us = project.duration() * 1_000_000.0;

        let mut child = match command("ffmpeg")
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => return send(RenderMsg::Failed(format!("Could not start ffmpeg: {e}"))),
        };

        let err_buf = Arc::new(Mutex::new(String::new()));
        if let Some(mut se) = child.stderr.take() {
            let eb = err_buf.clone();
            std::thread::spawn(move || {
                let mut s = String::new();
                let _ = se.read_to_string(&mut s);
                *eb.lock().unwrap() = s;
            });
        }

        if let Some(so) = child.stdout.take() {
            for line in BufReader::new(so).lines().map_while(Result::ok) {
                if cancel_t.load(Ordering::Relaxed) {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = std::fs::remove_file(&out);
                    return send(RenderMsg::Cancelled);
                }
                if let Some(v) = line.strip_prefix("out_time_us=") {
                    if let Ok(us) = v.trim().parse::<f64>() {
                        send(RenderMsg::Progress((us / total_us).clamp(0.0, 1.0) as f32));
                    }
                }
            }
        }

        match child.wait() {
            Ok(st) if st.success() => send(RenderMsg::Done),
            _ => {
                std::thread::sleep(std::time::Duration::from_millis(200));
                let e = err_buf.lock().unwrap().clone();
                let tail: String = e.lines().rev().take(12).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
                send(RenderMsg::Failed(if tail.is_empty() { "ffmpeg failed.".to_string() } else { tail }));
            }
        }
    });
    (rx, cancel)
}
