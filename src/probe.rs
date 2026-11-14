use crate::model::{MediaInfo, MediaItem, MediaKind, IMAGE_DEFAULT_SECS};
use crate::util::command;
use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Stdio;

/// Inspect a file with ffprobe. Handles any container/codec FFmpeg understands.
pub fn probe(path: &Path) -> Result<MediaItem> {
    let out = command("ffprobe")
        .args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"])
        .arg(path)
        .stdin(Stdio::null())
        .output()
        .context("could not run ffprobe (place ffmpeg.exe / ffprobe.exe next to reelforge.exe or on PATH)")?;
    if !out.status.success() {
        bail!("ffprobe could not read this file: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    let streams = v["streams"].as_array().cloned().unwrap_or_default();

    let is_cover_art = |s: &serde_json::Value| s["disposition"]["attached_pic"].as_i64() == Some(1);
    let video = streams
        .iter()
        .find(|s| s["codec_type"] == "video" && !is_cover_art(s));
    let audio = streams.iter().find(|s| s["codec_type"] == "audio");

    let format_name = v["format"]["format_name"].as_str().unwrap_or("");
    let fmt_duration = v["format"]["duration"]
        .as_str()
        .and_then(|d| d.parse::<f64>().ok())
        .filter(|d| *d > 0.0);

    let looks_like_image = video.is_some()
        && audio.is_none()
        && (format_name == "image2" || format_name.ends_with("_pipe") || fmt_duration.is_none());

    let kind = if video.is_none() && audio.is_some() {
        MediaKind::Audio
    } else if looks_like_image {
        MediaKind::Image
    } else if video.is_some() {
        MediaKind::Video
    } else {
        bail!("no audio or video streams found");
    };

    let (width, height) = video
        .map(|s| (s["width"].as_u64().unwrap_or(0) as u32, s["height"].as_u64().unwrap_or(0) as u32))
        .unwrap_or((0, 0));

    let duration = match kind {
        MediaKind::Image => IMAGE_DEFAULT_SECS,
        _ => fmt_duration.unwrap_or(0.0),
    };
    if duration <= 0.0 {
        bail!("could not determine duration");
    }

    Ok(MediaItem {
        id: 0,
        path: path.to_path_buf(),
        name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        kind,
        duration,
        width,
        height,
        has_audio: audio.is_some(),
        video_codec: video.and_then(|s| s["codec_name"].as_str()).unwrap_or("").to_string(),
        audio_codec: audio.and_then(|s| s["codec_name"].as_str()).unwrap_or("").to_string(),
        tag: String::new(),
        bin: String::new(),
        proxy: None,
        info: MediaInfo {
            size_bytes: v["format"]["size"].as_str().and_then(|x| x.parse().ok()).or_else(|| std::fs::metadata(path).ok().map(|m| m.len())).unwrap_or(0),
            fps: video.and_then(|s| s["avg_frame_rate"].as_str()).map(parse_rate).unwrap_or(0.0),
            bitrate_kbps: v["format"]["bit_rate"].as_str().and_then(|x| x.parse::<u64>().ok()).map(|b| (b / 1000) as u32).unwrap_or(0),
            modified: std::fs::metadata(path).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0),
            lut: String::new(),
        },
    })
}

/// "30000/1001" -> 29.97
fn parse_rate(s: &str) -> f64 {
    match s.split_once('/') {
        Some((a, b)) => {
            let (a, b): (f64, f64) = (a.parse().unwrap_or(0.0), b.parse().unwrap_or(0.0));
            if b > 0.0 {
                a / b
            } else {
                0.0
            }
        }
        None => s.parse().unwrap_or(0.0),
    }
}
