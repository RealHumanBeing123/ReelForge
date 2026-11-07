use crate::model::{Clip, Effect, Keyframe, MediaKind, Project, TitlePos, TitleSpec, Transition, TransitionKind};
use crate::util::command;
use eframe::egui;
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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
                e = format!(
                    "if(lt({var},{:.4}),{:.4}+({:.4})*({var}-{:.4})/{:.4},{e})",
                    w[1].t,
                    w[0].v,
                    w[1].v - w[0].v,
                    w[0].t,
                    span
                );
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

fn effect_filter(e: &Effect) -> String {
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
    }
}

/// Filters applied to one video layer (after its source), plus overlay x / y expressions.
fn layer_chain(c: &Clip, rw: u32, rh: u32, fps: u32, fit: bool) -> (Vec<String>, String, String) {
    let mut v: Vec<String> = vec!["setpts=PTS-STARTPTS".to_string(), format!("fps={fps}")];
    if fit {
        v.push(format!("scale={rw}:{rh}:force_original_aspect_ratio=decrease:flags=lanczos"));
        v.push("setsar=1".to_string());
    }
    for e in &c.effects {
        v.push(effect_filter(e));
    }

    let off = c.anim_offset;
    // A playback window that starts inside the clip has already passed its fade-in.
    let tin = if off > 1e-6 { Transition::default() } else { c.transition_in };
    let tout = c.transition_out;
    let uses_fade = |t: &Transition| matches!(t.kind, TransitionKind::Fade | TransitionKind::Zoom);
    let keyed_alpha = !c.anim.opacity.is_empty();
    let chroma = c.effects.iter().any(|e| matches!(e, Effect::ChromaKey { .. }));
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

/// Build FFmpeg input arguments and the filter graph. Produces label [vout] when `video`
/// and [aout] when `audio`. Returns (input args, filter graph, total duration).
pub fn build_graph(project: &Project, rw: u32, rh: u32, fps: u32, video: bool, audio: bool) -> (Vec<String>, String, f64) {
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
            let next = format!("l{layer_no}");
            filters.push(format!(
                "[{current}][{lab}]overlay=x='{x}':y='{y}':eof_action=pass:enable='between(t,{},{})'[{next}]",
                f(c.start),
                f(c.end())
            ));
            current = next;
            continue;
        }

        let Some(m) = project.media(c.media_id) else { continue };
        let wants_v = video && m.kind != MediaKind::Audio && !track.hidden;
        let wants_a = audio && m.has_audio && m.kind != MediaKind::Image && !track.muted;
        if !wants_v && !wants_a {
            continue;
        }
        let path = m.path.to_string_lossy().into_owned();

        match m.kind {
            MediaKind::Image => {
                args.extend(["-loop".to_string(), "1".to_string(), "-framerate".to_string(), fps.to_string(), "-t".to_string(), f(c.duration), "-i".to_string(), path]);
            }
            _ => {
                args.extend(["-ss".to_string(), f(c.in_point), "-t".to_string(), f(c.duration), "-i".to_string(), path]);
            }
        }
        let i = input_idx;
        input_idx += 1;

        if wants_v {
            let (chain, x, y) = layer_chain(c, rw, rh, fps, true);
            filters.push(format!("[{i}:v]{}[v{i}]", chain.join(",")));
            layer_no += 1;
            let next = format!("l{layer_no}");
            filters.push(format!(
                "[{current}][v{i}]overlay=x='{x}':y='{y}':eof_action=pass:enable='between(t,{},{})'[{next}]",
                f(c.start),
                f(c.end())
            ));
            current = next;
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
            filters.push(format!(
                "[{i}:a]aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo,asetpts=PTS-STARTPTS,volume={:.3},{fades}adelay={ms}|{ms}[a{i}]",
                c.volume
            ));
            audio_labels.push(format!("[a{i}]"));
        }
    }

    if video {
        filters.push(format!("[{current}]format=yuv420p[vout]"));
    }
    if audio {
        // A silent bed guarantees an audio stream of the full length, even with no audio clips.
        filters.push(format!("anullsrc=r=48000:cl=stereo,atrim=duration={}[asil]", f(total)));
        let n = audio_labels.len() + 1;
        filters.push(format!(
            "[asil]{}amix=inputs={n}:duration=longest:normalize=0,alimiter=limit=0.95[aout]",
            audio_labels.join("")
        ));
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

    let (inputs, graph, _) = build_graph(project, rw, rh, fps, true, true);
    args.extend(inputs);
    args.extend(["-filter_complex".to_string(), graph]);
    args.extend(["-map".to_string(), "[vout]".to_string(), "-map".to_string(), "[aout]".to_string()]);

    args.extend(encoder_args(s, true, true, true));
    args.extend(["-t".to_string(), f(total)]);
    args.push(out.to_string_lossy().into_owned());
    Ok(args)
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
