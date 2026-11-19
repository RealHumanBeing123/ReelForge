//! Render farm over a shared folder: split a timeline into parts, let any machine render a part,
//! then join the parts. No UI or serde here. Jobs live in `<farm>/jobs/<id>/`:
//! `job.json`, `part_001.rforge`, `part_001.claim`, `part_001.done`, `part_001.failed`, `out/`.
use crate::util::command;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;

/// Frame-aligned (start, end) pairs covering 0..total in at most `n` parts.
pub fn split(total: f64, fps: u32, n: usize) -> Vec<(f64, f64)> {
    let fps = fps.max(1) as f64;
    let frames = (total * fps).round().max(1.0) as u64;
    let n = (n.max(1) as u64).min(frames);
    let mut out = vec![];
    for k in 0..n {
        let a = frames * k / n;
        let b = frames * (k + 1) / n;
        if b > a {
            out.push((a as f64 / fps, b as f64 / fps));
        }
    }
    out
}

pub fn part_name(k: usize) -> String {
    format!("part_{k:03}")
}
pub fn path_of(job: &Path, k: usize, ext: &str) -> PathBuf {
    job.join(format!("{}.{ext}", part_name(k)))
}

/// Atomically claim a part for this worker. Only one machine can create the claim file.
pub fn try_claim(job: &Path, k: usize, worker: &str) -> bool {
    match OpenOptions::new().write(true).create_new(true).open(path_of(job, k, "claim")) {
        Ok(mut f) => {
            let _ = writeln!(f, "{worker}");
            true
        }
        Err(_) => false,
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PartState {
    Waiting,
    Rendering,
    Done,
    Failed,
}

pub fn part_state(job: &Path, k: usize) -> PartState {
    if path_of(job, k, "done").exists() {
        PartState::Done
    } else if path_of(job, k, "failed").exists() {
        PartState::Failed
    } else if path_of(job, k, "claim").exists() {
        PartState::Rendering
    } else {
        PartState::Waiting
    }
}

/// Make failed parts available again.
pub fn retry_failed(job: &Path, n: usize) {
    for k in 1..=n {
        if part_state(job, k) == PartState::Failed {
            let _ = std::fs::remove_file(path_of(job, k, "failed"));
            let _ = std::fs::remove_file(path_of(job, k, "claim"));
        }
    }
}

fn concat_quote(p: &Path) -> String {
    format!("file '{}'", p.to_string_lossy().replace('\\', "/").replace('\'', "'\\''"))
}

/// Join rendered parts without re-encoding. All parts come from the same preset, so this is exact
/// for video; audio can have a tiny discontinuity at joins in some codecs.
pub fn concat(parts: &[PathBuf], out: &Path, list_dir: &Path) -> Result<(), String> {
    if parts.is_empty() {
        return Err("No parts to join.".into());
    }
    let list = list_dir.join("concat.txt");
    let text: String = parts.iter().map(|p| concat_quote(p) + "\n").collect();
    std::fs::write(&list, text).map_err(|e| e.to_string())?;
    let o = command("ffmpeg")
        .args(["-v", "error", "-nostdin", "-y", "-f", "concat", "-safe", "0", "-i"])
        .arg(&list)
        .args(["-c", "copy", "-movflags", "+faststart"])
        .arg(out)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Could not start ffmpeg: {e}"))?;
    if o.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("ffmpeg failed").to_string())
    }
}
