//! Text formats: SRT subtitles and CMX3600 EDL export. No UI or FFmpeg dependencies.
use crate::model::{Project, Subtitle};

fn srt_time(t: f64) -> String {
    let ms = (t.max(0.0) * 1000.0).round() as u64;
    format!("{:02}:{:02}:{:02},{:03}", ms / 3_600_000, (ms / 60_000) % 60, (ms / 1000) % 60, ms % 1000)
}

pub fn to_srt(subs: &[Subtitle]) -> String {
    let mut out = String::new();
    for (i, s) in subs.iter().enumerate() {
        out.push_str(&format!("{}\n{} --> {}\n{}\n\n", i + 1, srt_time(s.start), srt_time(s.end), s.text.trim()));
    }
    out
}

fn parse_time(s: &str) -> Option<f64> {
    let s = s.trim().replace(',', ".");
    let mut parts = s.split(':');
    let h: f64 = parts.next()?.trim().parse().ok()?;
    let m: f64 = parts.next()?.trim().parse().ok()?;
    let sec: f64 = parts.next()?.trim().parse().ok()?;
    Some(h * 3600.0 + m * 60.0 + sec)
}

/// Parse an SRT file. Tolerates BOMs, CRLF line endings and missing index lines.
pub fn parse_srt(text: &str) -> Vec<Subtitle> {
    let text = text.trim_start_matches('\u{feff}').replace("\r\n", "\n").replace('\r', "\n");
    let mut out = vec![];
    for block in text.split("\n\n") {
        let mut lines = block.lines().filter(|l| !l.trim().is_empty()).peekable();
        let mut timing = None;
        while let Some(l) = lines.next() {
            if let Some((a, b)) = l.split_once("-->") {
                // ignore position tags after the end time
                let b = b.trim().split_whitespace().next().unwrap_or("");
                if let (Some(s), Some(e)) = (parse_time(a), parse_time(b)) {
                    timing = Some((s, e));
                }
                break;
            }
        }
        if let Some((start, end)) = timing {
            let body: Vec<&str> = lines.collect();
            if !body.is_empty() {
                out.push(Subtitle { start, end, text: body.join("\n") });
            }
        }
    }
    out
}

fn tc(t: f64, fps: u32) -> String {
    let fps = fps.max(1) as u64;
    let frames = (t.max(0.0) * fps as f64).round() as u64;
    format!("{:02}:{:02}:{:02}:{:02}", frames / (fps * 3600), (frames / (fps * 60)) % 60, (frames / fps) % 60, frames % fps)
}

fn reel(name: &str) -> String {
    let mut r: String = name.chars().filter(|c| c.is_ascii_alphanumeric()).take(8).collect::<String>().to_uppercase();
    if r.is_empty() {
        r = "AX".to_string();
    }
    r
}

/// CMX3600 EDL of the video clips (V) and audio clips (A), in timeline order.
pub fn to_edl(project: &Project, title: &str) -> String {
    let mut out = format!("TITLE: {}\nFCM: NON-DROP FRAME\n\n", title);
    let mut clips: Vec<_> = project.clips.iter().filter(|c| c.title.is_none()).collect();
    clips.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap().then(a.track.cmp(&b.track)));
    for (n, c) in clips.iter().enumerate() {
        let Some(m) = project.media(c.media_id) else { continue };
        let kind = match m.kind {
            crate::model::MediaKind::Audio => "A",
            _ => "V",
        };
        let span = c.duration * c.speed;
        let (a, b) = if c.freeze { (c.in_point, c.in_point + c.duration) } else { (c.in_point, c.in_point + span) };
        out.push_str(&format!("{:03}  {:<8} {:<5} C        {} {} {} {}\n", n + 1, reel(&m.name), kind, tc(a, project.fps), tc(b, project.fps), tc(c.start, project.fps), tc(c.end(), project.fps)));
        out.push_str(&format!("* FROM CLIP NAME: {}\n", m.name));
        if (c.speed - 1.0).abs() > 1e-6 && !c.freeze {
            out.push_str(&format!("M2   {:<8} {:>6.1}  {}\n", reel(&m.name), project.fps as f64 * c.speed * if c.reverse { -1.0 } else { 1.0 }, tc(a, project.fps)));
        }
        out.push('\n');
    }
    out
}
