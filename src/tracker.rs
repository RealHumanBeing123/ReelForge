//! Point tracking: follow one spot of a video by template matching on low-resolution frames.
//!
//! FFmpeg decodes grey frames; each new frame is searched around the last position for the patch
//! that best matches (lowest sum of absolute differences) the patch seen in the previous frame.
//! No UI or egui dependencies.
use crate::model::{Ease, Keyframe, PinKey};
use crate::util::command;
use std::io::Read;
use std::path::Path;
use std::process::Stdio;

const W: usize = 480;
const SAMPLE_FPS: f64 = 12.0;
const R: i32 = 12; // patch half-size
const S: i32 = 22; // search radius per frame (pixels at 480 wide)
/// Longest stretch tracked in one go (seconds of source).
pub const MAX_SECS: f64 = 60.0;

/// One tracked position: seconds of source time after the start point, and a 0..1 position in the picture.
#[derive(Clone, Copy, Debug)]
pub struct TrackPoint {
    pub dt: f64,
    pub x: f32,
    pub y: f32,
}

fn patch_sad(a: &[u8], ax: i32, ay: i32, b: &[u8], bx: i32, by: i32, h: usize) -> u32 {
    let mut sum = 0u32;
    for dy in -R..=R {
        let ra = ((ay + dy) as usize) * W;
        let rb = ((by + dy) as usize) * W;
        for dx in -R..=R {
            let va = a[ra + (ax + dx) as usize] as i32;
            let vb = b[rb + (bx + dx) as usize] as i32;
            sum += (va - vb).unsigned_abs();
        }
    }
    let _ = h;
    sum
}

/// Follow the grey frames in `frames` (each W x h) starting from `start` (pixels). Pure function.
pub fn follow(frames: &[Vec<u8>], h: usize, start: (i32, i32)) -> Vec<(i32, i32)> {
    let clamp = |x: i32, hi: usize| x.clamp(R, hi as i32 - 1 - R);
    let mut pos = (clamp(start.0, W), clamp(start.1, h));
    let mut out = vec![pos];
    for k in 1..frames.len() {
        let (prev, cur) = (&frames[k - 1], &frames[k]);
        let mut best = (u32::MAX, pos);
        for dy in -S..=S {
            let y = pos.1 + dy;
            if y < R || y > h as i32 - 1 - R {
                continue;
            }
            for dx in -S..=S {
                let x = pos.0 + dx;
                if x < R || x > W as i32 - 1 - R {
                    continue;
                }
                let sad = patch_sad(prev, pos.0, pos.1, cur, x, y, h);
                // tie-break toward "no motion" so flat areas do not wander (too small to bias real motion)
                let sad = sad * 8 + (dx.abs() + dy.abs()) as u32;
                if sad < best.0 {
                    best = (sad, (x, y));
                }
            }
        }
        // Re-anchor: refine against the very first patch so small errors do not pile up.
        let first = &frames[0];
        let a0 = out[0];
        let mut refined = (u32::MAX, best.1);
        for dy in -3..=3 {
            for dx in -3..=3 {
                let (x, y) = (best.1 .0 + dx, best.1 .1 + dy);
                if x < R || x > W as i32 - 1 - R || y < R || y > h as i32 - 1 - R {
                    continue;
                }
                let sad = patch_sad(prev, pos.0, pos.1, cur, x, y, h) + patch_sad(first, a0.0, a0.1, cur, x, y, h);
                if sad < refined.0 {
                    refined = (sad, (x, y));
                }
            }
        }
        pos = refined.1;
        out.push(pos);
    }
    out
}

/// Decode `secs` seconds of `path` from `from` and track the point `start` (0..1 of the picture).
pub fn track_point(path: &Path, from: f64, secs: f64, src_w: u32, src_h: u32, start: (f32, f32)) -> Result<Vec<TrackPoint>, String> {
    if src_w == 0 || src_h == 0 {
        return Err("The video size is unknown.".into());
    }
    let secs = secs.clamp(0.1, MAX_SECS);
    let h = (((W as f64) * src_h as f64 / src_w as f64).round() as usize).max(2) & !1;
    let mut child = command("ffmpeg")
        .args(["-v", "error", "-nostdin", "-ss", &format!("{:.3}", from.max(0.0)), "-t", &format!("{secs:.3}"), "-i"])
        .arg(path)
        .args(["-vf", &format!("fps={SAMPLE_FPS},scale={W}:{h},format=gray"), "-f", "rawvideo", "-pix_fmt", "gray", "-"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Could not start ffmpeg: {e}"))?;
    let mut out = child.stdout.take().ok_or("no output from ffmpeg")?;
    let mut frames: Vec<Vec<u8>> = vec![];
    loop {
        let mut buf = vec![0u8; W * h];
        match out.read_exact(&mut buf) {
            Ok(()) => frames.push(buf),
            Err(_) => break,
        }
    }
    let _ = child.wait();
    if frames.len() < 2 {
        return Err("Not enough video to track.".into());
    }
    let px = ((start.0 * W as f32).round() as i32, (start.1 * h as f32).round() as i32);
    let path_px = follow(&frames, h, px);
    Ok(path_px
        .iter()
        .enumerate()
        .map(|(k, (x, y))| TrackPoint { dt: k as f64 / SAMPLE_FPS, x: *x as f32 / W as f32, y: *y as f32 / h as f32 })
        .collect())
}

/// Convert a tracked picture position (0..1 of the source picture) to a canvas fraction (0..1),
/// for a picture fitted inside the canvas.
pub fn pic_to_canvas(p: (f32, f32), src: (u32, u32), canvas: (u32, u32)) -> (f32, f32) {
    let s = (canvas.0 as f32 / src.0 as f32).min(canvas.1 as f32 / src.1 as f32);
    let (pw, ph) = (src.0 as f32 * s / canvas.0 as f32, src.1 as f32 * s / canvas.1 as f32);
    (0.5 + (p.0 - 0.5) * pw, 0.5 + (p.1 - 0.5) * ph)
}

/// Inverse of `pic_to_canvas`.
pub fn canvas_to_pic(c: (f32, f32), src: (u32, u32), canvas: (u32, u32)) -> (f32, f32) {
    let s = (canvas.0 as f32 / src.0 as f32).min(canvas.1 as f32 / src.1 as f32);
    let (pw, ph) = (src.0 as f32 * s / canvas.0 as f32, src.1 as f32 * s / canvas.1 as f32);
    (0.5 + (c.0 - 0.5) / pw.max(1e-3), 0.5 + (c.1 - 0.5) / ph.max(1e-3))
}

/// Keyframes (x, y) that centre a layer on the tracked point. `t_offset` is the target clip time
/// of the first track point (may be negative); `speed` is the source clip's speed. Values are
/// canvas offsets from the centre, as used by position keyframes. Thinned to about 5 per second.
pub fn to_keyframes(points: &[TrackPoint], src: (u32, u32), canvas: (u32, u32), t_offset: f64, speed: f64, target_len: f64) -> (Vec<Keyframe>, Vec<Keyframe>) {
    // light smoothing (3-point average) to hide pixel jitter
    let n = points.len();
    let sm: Vec<(f64, f32, f32)> = (0..n)
        .map(|i| {
            let (a, b) = (i.saturating_sub(1), (i + 1).min(n - 1));
            let cnt = (b - a + 1) as f32;
            let (sx, sy) = points[a..=b].iter().fold((0.0, 0.0), |s, p| (s.0 + p.x, s.1 + p.y));
            (points[i].dt / speed.max(0.01) + t_offset, sx / cnt, sy / cnt)
        })
        .collect();
    let (mut kx, mut ky) = (vec![], vec![]);
    let mut last_t = f64::NEG_INFINITY;
    for (i, (t, x, y)) in sm.iter().enumerate() {
        if *t < 0.0 || *t > target_len + 1e-6 {
            continue;
        }
        if *t - last_t < 0.2 && i + 1 != n {
            continue;
        }
        last_t = *t;
        let (cx, cy) = pic_to_canvas((*x, *y), src, canvas);
        kx.push(Keyframe { t: *t, v: (cx - 0.5) as f64, ease: Ease::Linear });
        ky.push(Keyframe { t: *t, v: (cy - 0.5) as f64, ease: Ease::Linear });
    }
    // hold the first position before tracking began inside the target
    if let (Some(fx), Some(fy)) = (kx.first().cloned(), ky.first().cloned()) {
        if fx.t > 0.01 {
            kx.insert(0, Keyframe { t: 0.0, ..fx });
            ky.insert(0, Keyframe { t: 0.0, ..fy });
        }
    }
    (kx, ky)
}

/// Track four corners (TL, TR, BR, BL; 0..1 of the picture) one after the other.
pub fn track_corners(path: &Path, from: f64, secs: f64, src_w: u32, src_h: u32, corners: [(f32, f32); 4]) -> Result<[Vec<TrackPoint>; 4], String> {
    let mut out: [Vec<TrackPoint>; 4] = Default::default();
    for (i, c) in corners.iter().enumerate() {
        out[i] = track_point(path, from, secs, src_w, src_h, *c)?;
    }
    Ok(out)
}

/// Corner-pin keys from four tracked corners (canvas fractions). Same timing rules as
/// `to_keyframes`; the first key is held back to the start of the target clip.
pub fn to_pin_keys(tracks: &[Vec<TrackPoint>; 4], src: (u32, u32), canvas: (u32, u32), t_offset: f64, speed: f64, target_len: f64) -> Vec<PinKey> {
    let n = tracks.iter().map(|t| t.len()).min().unwrap_or(0);
    let mut keys: Vec<PinKey> = vec![];
    let mut last_t = f64::NEG_INFINITY;
    for i in 0..n {
        let t = tracks[0][i].dt / speed.max(0.01) + t_offset;
        if t < 0.0 || t > target_len + 1e-6 || (t - last_t < 0.15 && i + 1 != n) {
            continue;
        }
        last_t = t;
        let mut pts = [[0.0f32; 2]; 4];
        for c in 0..4 {
            // light 3-point smoothing against pixel jitter
            let (a, b) = (i.saturating_sub(1), (i + 1).min(n - 1));
            let cnt = (b - a + 1) as f32;
            let (sx, sy) = tracks[c][a..=b].iter().fold((0.0, 0.0), |s, p| (s.0 + p.x, s.1 + p.y));
            let q = pic_to_canvas((sx / cnt, sy / cnt), src, canvas);
            pts[c] = [q.0, q.1];
        }
        keys.push(PinKey { t, pts });
    }
    if let Some(f) = keys.first().cloned() {
        if f.t > 0.01 {
            keys.insert(0, PinKey { t: 0.0, pts: f.pts });
        }
    }
    keys
}
