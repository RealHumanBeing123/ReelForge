//! Text formats: SRT subtitles, CMX3600 EDL, Final Cut Pro 7 XML (xmeml) and OpenTimelineIO export. No UI or FFmpeg dependencies.
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

/// UTC "YYYY-MM-DD HH:MM:SS" from Unix seconds.
pub fn iso_utc(secs: u64) -> String {
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    // civil-from-days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}", rem / 3600, (rem / 60) % 60, rem % 60)
}

fn csv_field(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// CSV of everything imported into the project.
pub fn to_ingest_csv(project: &Project) -> String {
    let mut out = String::from("imported_utc,name,type,duration_s,size_bytes,path\n");
    for e in &project.ingest {
        out.push_str(&format!("{},{},{},{:.3},{},{}\n", iso_utc(e.when), csv_field(&e.name), e.kind, e.duration, e.size_bytes, csv_field(&e.path)));
    }
    out
}

// ---------------------------------------------------------------- FCP7 XML (xmeml)

fn xml_esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// `file://localhost/C%3a/Users/...` style URL used by xmeml.
pub fn path_url(path: &str) -> String {
    let p = path.replace('\\', "/");
    let mut out = String::from("file://localhost");
    if !p.starts_with('/') {
        out.push('/');
    }
    for b in p.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'.' | b'-' | b'_' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{:02x}", b)),
        }
    }
    out
}

fn url_path(url: &str) -> String {
    let rest = url.strip_prefix("file://localhost").or_else(|| url.strip_prefix("file://")).unwrap_or(url);
    let b = rest.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&rest[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    let s = String::from_utf8_lossy(&out).into_owned();
    // "/C:/x" -> "C:/x"
    if s.len() > 2 && s.as_bytes()[0] == b'/' && s.as_bytes()[2] == b':' {
        s[1..].to_string()
    } else {
        s
    }
}

/// Final Cut Pro 7 XML (xmeml v5), readable by Premiere Pro, DaVinci Resolve and Final Cut.
/// Video clips go on video tracks (one per timeline track) with linked audio on audio tracks.
/// Speed changes are written as a Time Remap speed parameter. Titles and particles are skipped.
pub fn to_xmeml(project: &Project, title: &str) -> String {
    use crate::model::MediaKind;
    let fps = project.fps.max(1);
    let fr = |t: f64| (t.max(0.0) * fps as f64).round() as i64;
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE xmeml>\n<xmeml version=\"5\">\n<sequence id=\"sequence-1\">\n");
    out.push_str(&format!("<name>{}</name>\n<duration>{}</duration>\n<rate><timebase>{fps}</timebase><ntsc>FALSE</ntsc></rate>\n", xml_esc(title), fr(project.duration())));
    out.push_str("<media>\n<video>\n<format><samplecharacteristics>");
    out.push_str(&format!("<rate><timebase>{fps}</timebase><ntsc>FALSE</ntsc></rate><width>{}</width><height>{}</height><pixelaspectratio>square</pixelaspectratio>", project.width, project.height));
    out.push_str("</samplecharacteristics></format>\n");
    let mut seen: Vec<u64> = vec![];
    let mut item_no = 0;
    let file_xml = |m: &crate::model::MediaItem, seen: &mut Vec<u64>| -> String {
        if seen.contains(&m.id) {
            return format!("<file id=\"file-{}\"/>", m.id);
        }
        seen.push(m.id);
        let mut s = format!("<file id=\"file-{}\"><name>{}</name><pathurl>{}</pathurl><rate><timebase>{fps}</timebase><ntsc>FALSE</ntsc></rate><duration>{}</duration><media>", m.id, xml_esc(&m.name), xml_esc(&path_url(&m.path.to_string_lossy())), fr(m.duration));
        if m.kind != MediaKind::Audio {
            s.push_str(&format!("<video><samplecharacteristics><width>{}</width><height>{}</height></samplecharacteristics></video>", m.width, m.height));
        }
        if m.has_audio || m.kind == MediaKind::Audio {
            s.push_str("<audio><samplecharacteristics><depth>16</depth><samplerate>48000</samplerate></samplecharacteristics><channelcount>2</channelcount></audio>");
        }
        s.push_str("</media></file>");
        s
    };
    let item = |c: &crate::model::Clip, m: &crate::model::MediaItem, file: &str, n: usize, audio: bool| -> String {
        let span = if c.freeze { c.duration } else { c.duration * c.speed };
        let (a, b) = (fr(c.in_point), fr(c.in_point + span));
        let mut s = format!(
            "<clipitem id=\"{}-{n}\"><name>{}</name><enabled>TRUE</enabled><duration>{}</duration><rate><timebase>{fps}</timebase><ntsc>FALSE</ntsc></rate><start>{}</start><end>{}</end><in>{a}</in><out>{b}</out>{file}",
            if audio { "clipitem-a" } else { "clipitem" },
            xml_esc(&m.name),
            fr(m.duration),
            fr(c.start),
            fr(c.end())
        );
        if audio {
            s.push_str("<sourcetrack><mediatype>audio</mediatype><trackindex>1</trackindex></sourcetrack>");
        }
        if !c.freeze && ((c.speed - 1.0).abs() > 1e-6 || c.reverse) {
            let pct = c.speed * 100.0 * if c.reverse { -1.0 } else { 1.0 };
            s.push_str(&format!("<filter><effect><name>Time Remap</name><effectid>timeremap</effectid><effectcategory>motion</effectcategory><effecttype>motion</effecttype><mediatype>video</mediatype><parameter><parameterid>speed</parameterid><name>speed</name><value>{pct:.3}</value></parameter></effect></filter>"));
        }
        s.push_str("</clipitem>");
        s
    };
    let ntracks = project.tracks.len();
    let mut audio_items: Vec<Vec<String>> = vec![vec![]; ntracks];
    for t in 0..ntracks {
        let mut clips: Vec<_> = project.clips.iter().filter(|c| c.track == t && !c.generated()).collect();
        clips.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap());
        out.push_str("<track>\n");
        for c in clips {
            let Some(m) = project.media(c.media_id) else { continue };
            item_no += 1;
            let file = file_xml(m, &mut seen);
            if m.kind != MediaKind::Audio {
                out.push_str(&item(c, m, &file, item_no, false));
                out.push('\n');
            }
            if (m.has_audio || m.kind == MediaKind::Audio) && m.kind != MediaKind::Image && !c.freeze {
                let afile = if m.kind == MediaKind::Audio { file.clone() } else { format!("<file id=\"file-{}\"/>", m.id) };
                audio_items[t].push(item(c, m, &afile, item_no, true));
            }
        }
        out.push_str("</track>\n");
    }
    out.push_str("</video>\n<audio>\n");
    for t in audio_items {
        out.push_str("<track>\n");
        for i in t {
            out.push_str(&i);
            out.push('\n');
        }
        out.push_str("</track>\n");
    }
    out.push_str("</audio>\n</media>\n</sequence>\n</xmeml>\n");
    out
}

/// One clip read from an xmeml file.
#[derive(Debug, Clone, PartialEq)]
pub struct XmlClip {
    pub name: String,
    pub path: String,
    /// 0-based video track index (or audio track index when `audio`).
    pub track: usize,
    pub audio: bool,
    pub start: f64,
    pub in_pt: f64,
    pub out_pt: f64,
    /// Playback speed (1 = normal), negative if reversed.
    pub speed: f64,
}

fn tag_value<'a>(s: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let i = s.find(&open)? + open.len();
    let j = s[i..].find(&format!("</{tag}>"))? + i;
    Some(&s[i..j])
}

fn unesc(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&amp;", "&")
}

/// Byte range of the `<tag>...</tag>` element starting at or after `from`, matching nested tags
/// of the same name (a clip's `<file>` also contains `<video>` / `<audio>`).
fn section(s: &str, tag: &str, from: usize) -> Option<(usize, usize)> {
    let (open, close) = (format!("<{tag}>"), format!("</{tag}>"));
    let a = from + s[from..].find(&open)?;
    let mut depth = 0i32;
    let mut i = a;
    loop {
        let o = s[i..].find(&open).map(|v| v + i);
        let c = s[i..].find(&close).map(|v| v + i)?;
        match o {
            Some(o) if o < c => {
                depth += 1;
                i = o + open.len();
            }
            _ => {
                depth -= 1;
                i = c + close.len();
                if depth == 0 {
                    return Some((a, i));
                }
            }
        }
    }
}

/// Read clips from an FCP7 XML sequence. Only what this editor can use: file, in/out, start, speed.
/// Returns (fps, clips).
pub fn parse_xmeml(text: &str) -> Option<(u32, Vec<XmlClip>)> {
    let seq = &text[text.find("<sequence")?..];
    let fps: u32 = tag_value(seq, "timebase")?.trim().parse().ok()?;
    let f = fps.max(1) as f64;
    let mut clips = vec![];
    let mut files: std::collections::HashMap<String, (String, String)> = Default::default(); // id -> (path, name)
    let mut from = 0usize;
    for (audio, tag) in [(false, "video"), (true, "audio")] {
        let Some((a, b)) = section(seq, tag, from) else { continue };
        from = b;
        let sect = &seq[a..b];
        let mut track = 0usize;
        let mut rest = sect;
        while let Some(ts) = rest.find("<track") {
            let after = &rest[ts..];
            let Some(te) = after.find("</track>") else { break };
            let block = &after[..te];
            let mut cur = block;
            while let Some(cs) = cur.find("<clipitem") {
                let after = &cur[cs..];
                let Some(ce) = after.find("</clipitem>") else { break };
                let item = &after[..ce];
                cur = &after[ce + 11..];
                // file reference (full definition or id only)
                let (mut path, mut fname) = (String::new(), String::new());
                if let Some(fs) = item.find("<file") {
                    let fi = &item[fs..];
                    let id = fi.split('"').nth(1).unwrap_or("").to_string();
                    if let Some(u) = tag_value(fi, "pathurl") {
                        path = url_path(&unesc(u));
                        fname = tag_value(fi, "name").map(unesc).unwrap_or_default();
                        files.insert(id, (path.clone(), fname.clone()));
                    } else if let Some((p, n)) = files.get(&id) {
                        path = p.clone();
                        fname = n.clone();
                    }
                }
                let num = |t: &str| tag_value(item, t).and_then(|v| v.trim().parse::<f64>().ok());
                let (Some(start), Some(i), Some(o)) = (num("start"), num("in"), num("out")) else { continue };
                if start < 0.0 {
                    continue; // transition placeholder
                }
                let mut speed = 1.0;
                if let Some(p) = item.find("<parameterid>speed</parameterid>") {
                    if let Some(v) = tag_value(&item[p..], "value").and_then(|v| v.trim().parse::<f64>().ok()) {
                        if v.abs() > 0.01 {
                            speed = v / 100.0;
                        }
                    }
                }
                let name = tag_value(item, "name").map(unesc).unwrap_or(fname);
                clips.push(XmlClip { name, path, track, audio, start: start / f, in_pt: i / f, out_pt: o / f, speed });
            }
            track += 1;
            rest = &after[te + 8..];
        }
    }
    Some((fps, clips))
}

// ---------------------------------------------------------------- OpenTimelineIO

fn js(s: &str) -> String {
    let mut o = String::from("\"");
    for ch in s.chars() {
        match ch {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn rt(v: f64, rate: u32) -> String {
    format!("{{\"OTIO_SCHEMA\": \"RationalTime.1\", \"rate\": {rate}.0, \"value\": {:.3}}}", v * rate as f64)
}

/// OpenTimelineIO (.otio) JSON: one video track per timeline track (plus matching audio tracks).
pub fn to_otio(project: &Project, title: &str) -> String {
    use crate::model::MediaKind;
    let fps = project.fps.max(1);
    let mut tracks: Vec<String> = vec![];
    for (t, tr) in project.tracks.iter().enumerate() {
        for audio in [false, true] {
            let mut clips: Vec<_> = project.clips.iter().filter(|c| c.track == t && !c.generated()).collect();
            clips.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap());
            let mut kids: Vec<String> = vec![];
            let mut cursor = 0.0f64;
            for c in clips {
                let Some(m) = project.media(c.media_id) else { continue };
                let has = if audio { (m.has_audio || m.kind == MediaKind::Audio) && m.kind != MediaKind::Image && !c.freeze } else { m.kind != MediaKind::Audio };
                if !has {
                    continue;
                }
                if c.start > cursor + 1e-6 {
                    kids.push(format!("{{\"OTIO_SCHEMA\": \"Gap.1\", \"name\": \"\", \"source_range\": {{\"OTIO_SCHEMA\": \"TimeRange.1\", \"start_time\": {}, \"duration\": {}}}}}", rt(0.0, fps), rt(c.start - cursor, fps)));
                }
                let span = if c.freeze { c.duration } else { c.duration * c.speed };
                let mut fx = String::new();
                if !c.freeze && ((c.speed - 1.0).abs() > 1e-6 || c.reverse) {
                    fx = format!("{{\"OTIO_SCHEMA\": \"LinearTimeWarp.1\", \"name\": \"Speed\", \"effect_name\": \"LinearTimeWarp\", \"time_scalar\": {:.4}}}", c.speed * if c.reverse { -1.0 } else { 1.0 });
                }
                kids.push(format!(
                    "{{\"OTIO_SCHEMA\": \"Clip.1\", \"name\": {}, \"source_range\": {{\"OTIO_SCHEMA\": \"TimeRange.1\", \"start_time\": {}, \"duration\": {}}}, \"effects\": [{}], \"media_reference\": {{\"OTIO_SCHEMA\": \"ExternalReference.1\", \"target_url\": {}, \"available_range\": {{\"OTIO_SCHEMA\": \"TimeRange.1\", \"start_time\": {}, \"duration\": {}}}}}}}",
                    js(&m.name),
                    rt(c.in_point, fps),
                    rt(span, fps),
                    fx,
                    js(&path_url(&m.path.to_string_lossy())),
                    rt(0.0, fps),
                    rt(m.duration, fps)
                ));
                cursor = c.end();
            }
            if kids.is_empty() {
                continue;
            }
            tracks.push(format!("{{\"OTIO_SCHEMA\": \"Track.1\", \"name\": {}, \"kind\": \"{}\", \"children\": [{}]}}", js(&tr.name), if audio { "Audio" } else { "Video" }, kids.join(", ")));
        }
    }
    format!(
        "{{\"OTIO_SCHEMA\": \"Timeline.1\", \"name\": {}, \"global_start_time\": {}, \"tracks\": {{\"OTIO_SCHEMA\": \"Stack.1\", \"name\": \"tracks\", \"children\": [{}]}}}}\n",
        js(title),
        rt(0.0, fps),
        tracks.join(", ")
    )
}
