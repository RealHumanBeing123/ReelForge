//! Procedural particle layers. Every particle's position is a closed-form function of time, so any
//! frame (and so any playback or export window) renders independently and identically.
//! Frames are rasterised in software to a raw RGBA file that FFmpeg reads as an input.
use crate::model::{ParticleKind, ParticleSpec};
use std::io::Write;
use std::path::PathBuf;

/// Particle layers are drawn at reduced size and scaled up (they are soft, so it is invisible).
pub fn raw_size(rw: u32, rh: u32) -> (u32, u32) {
    let max_w = 480.0;
    let s = (max_w / rw.max(1) as f64).min(1.0);
    (((rw as f64 * s).round() as u32).max(16) & !1, ((rh as f64 * s).round() as u32).max(16) & !1)
}

fn hash(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846ca68b);
    x ^= x >> 16;
    x
}
fn rnd(seed: u32, i: u32, k: u32) -> f32 {
    hash(seed.wrapping_mul(0x9e3779b1) ^ i.wrapping_mul(0x85ebca6b) ^ k.wrapping_mul(0xc2b2ae35)) as f32 / u32::MAX as f32
}

struct P {
    x: f32,
    y: f32,
    r: f32,
    a: f32,
    c: [u8; 3],
    streak: Option<(f32, f32)>,
}

fn wrap(v: f32) -> f32 {
    v - v.floor()
}

/// Particles visible at time `t` (positions normalised 0..1, radii as a fraction of height).
fn particles_at(s: &ParticleSpec, t: f32) -> Vec<P> {
    let mut out = Vec::with_capacity(s.count as usize);
    let sp = s.speed.max(0.0);
    for i in 0..s.count {
        let (r0, r1, r2, r3) = (rnd(s.seed, i, 1), rnd(s.seed, i, 2), rnd(s.seed, i, 3), rnd(s.seed, i, 4));
        match s.kind {
            ParticleKind::Snow => {
                let fall = (0.05 + 0.10 * r2) * sp;
                let y = wrap(r1 + t * fall);
                let x = wrap(r0 + (t * (0.6 + r3) * 0.7 + r2 * 6.0).sin() * 0.02 * (0.5 + r3) + t * 0.01 * sp);
                out.push(P { x, y, r: (0.003 + 0.007 * r3) * s.size, a: 0.5 + 0.5 * r2, c: s.color, streak: None });
            }
            ParticleKind::Rain => {
                let fall = (0.9 + 0.5 * r2) * sp;
                let y = wrap(r1 + t * fall);
                let x = wrap(r0 - y * 0.05);
                out.push(P { x, y, r: 0.0012 * s.size, a: 0.25 + 0.5 * r3, c: s.color, streak: Some((-0.05 * 0.2, 0.035 * s.size)) });
            }
            ParticleKind::Sparks => {
                // each spark is born at the bottom, flies up in an arc, fades, and respawns
                let life = 1.2 + 1.5 * r2;
                let phase = wrap(t * sp / life + r1);
                let n = (t * sp / life + r1).floor() as i32;
                let (b0, b1) = (rnd(s.seed, i, 10 + n as u32), rnd(s.seed, i, 40 + n as u32));
                let x0 = b0;
                let vx = (b1 - 0.5) * 0.5;
                let age = phase * life;
                let x = x0 + vx * age * 0.6;
                let y = 1.05 - (0.55 + 0.4 * r3) * age + 0.25 * age * age;
                if !(0.0..=1.0).contains(&x) {
                    continue;
                }
                out.push(P { x, y, r: (0.002 + 0.004 * r3) * s.size, a: (1.0 - phase).powf(1.5), c: s.color, streak: None });
            }
            ParticleKind::Confetti => {
                let fall = (0.12 + 0.12 * r2) * sp;
                let y = wrap(r1 + t * fall);
                let x = wrap(r0 + (t * (1.0 + r3) + r2 * 9.0).sin() * 0.03);
                // hue from hash
                let h = r3 * 6.0;
                let f = h - h.floor();
                let (q, tt) = (1.0 - f, f);
                let (rr, gg, bb) = match h as u32 % 6 {
                    0 => (1.0, tt, 0.0),
                    1 => (q, 1.0, 0.0),
                    2 => (0.0, 1.0, tt),
                    3 => (0.0, q, 1.0),
                    4 => (tt, 0.0, 1.0),
                    _ => (1.0, 0.0, q),
                };
                // flutter: width shrinks as the piece turns edge-on
                let flip = ((t * (2.0 + 3.0 * r2) + r0 * 20.0).sin()).abs().max(0.15);
                out.push(P { x, y, r: 0.006 * s.size, a: 1.0, c: [(rr * 255.0) as u8, (gg * 255.0) as u8, (bb * 255.0) as u8], streak: Some((0.0, flip)) });
            }
            ParticleKind::Fireflies => {
                let x = wrap(r0 + (t * (0.1 + 0.1 * r2) * sp + r3 * 6.0).sin() * 0.06);
                let y = wrap(r1 + (t * (0.08 + 0.1 * r3) * sp + r2 * 6.0).cos() * 0.05);
                let blink = 0.5 + 0.5 * (t * (0.8 + 1.5 * r2) * sp.max(0.1) + r1 * 20.0).sin();
                out.push(P { x, y, r: (0.008 + 0.01 * r3) * s.size, a: blink * blink, c: s.color, streak: None });
            }
        }
    }
    out
}

/// Render one frame of particles at time `t` into premultiplied-free straight RGBA (w*h*4).
pub fn render_frame(s: &ParticleSpec, t: f32, w: u32, h: u32) -> Vec<u8> {
    let (wf, hf) = (w as f32, h as f32);
    // accumulate premultiplied colour, then convert to straight alpha
    let mut acc = vec![0.0f32; (w * h * 4) as usize];
    for p in particles_at(s, t) {
        let (cx, cy) = (p.x * wf, p.y * hf);
        let rad = (p.r * hf).max(0.7);
        let (ax, ay) = match (s.kind, p.streak) {
            (ParticleKind::Rain, Some((_, len))) => (0.8, len * hf * 0.5 + rad),
            (ParticleKind::Confetti, Some((_, flip))) => (rad * 1.4 * flip, rad * 1.1),
            _ => (rad, rad),
        };
        let x0 = ((cx - ax - 1.0).floor().max(0.0)) as i32;
        let x1 = ((cx + ax + 1.0).ceil().min(wf - 1.0)) as i32;
        let y0 = ((cy - ay - 1.0).floor().max(0.0)) as i32;
        let y1 = ((cy + ay + 1.0).ceil().min(hf - 1.0)) as i32;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (dx, dy) = ((x as f32 + 0.5 - cx) / ax, (y as f32 + 0.5 - cy) / ay);
                let d2 = dx * dx + dy * dy;
                if d2 >= 1.0 {
                    continue;
                }
                let k = if s.kind == ParticleKind::Confetti { 1.0 } else { (1.0 - d2).powf(if s.kind == ParticleKind::Fireflies { 1.6 } else { 0.8 }) };
                let a = (p.a * k).clamp(0.0, 1.0);
                let o = ((y as u32 * w + x as u32) * 4) as usize;
                // "over" onto the accumulator
                let inv = 1.0 - a;
                for ch in 0..3 {
                    acc[o + ch] = acc[o + ch] * inv + (p.c[ch] as f32 / 255.0) * a;
                }
                acc[o + 3] = acc[o + 3] * inv + a;
            }
        }
    }
    let mut out = vec![0u8; acc.len()];
    for px in 0..(w * h) as usize {
        let a = acc[px * 4 + 3];
        if a > 0.0 {
            for ch in 0..3 {
                out[px * 4 + ch] = ((acc[px * 4 + ch] / a).clamp(0.0, 1.0) * 255.0).round() as u8;
            }
            out[px * 4 + 3] = (a.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
    out
}

fn spec_key(s: &ParticleSpec) -> u64 {
    let mut h = 1469598103934665603u64;
    let text = format!("{:?}", s);
    for b in text.bytes() {
        h = (h ^ b as u64).wrapping_mul(1099511628211);
    }
    h
}

/// Make sure the raw frames for this window exist (cached by content) and return the file.
/// `t0` is the time inside the clip that the first frame shows.
pub fn ensure_raw(s: &ParticleSpec, clip_id: u64, w: u32, h: u32, fps: u32, t0: f64, frames: u32) -> Option<PathBuf> {
    let frames = frames.min(60 * fps.max(1)); // cap one layer at a minute per render window
    let key = spec_key(s);
    let dir = std::env::temp_dir().join("reelforge_particles");
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join(format!("p{clip_id}_{key:016x}_{w}x{h}_{fps}_{:.0}ms_{frames}.rgba", t0 * 1000.0));
    if path.metadata().map_or(false, |m| m.len() == w as u64 * h as u64 * 4 * frames as u64) {
        return Some(path);
    }
    let tmp = path.with_extension("part");
    let mut f = std::io::BufWriter::new(std::fs::File::create(&tmp).ok()?);
    for n in 0..frames {
        let t = (t0 + n as f64 / fps.max(1) as f64) as f32;
        f.write_all(&render_frame(s, t, w, h)).ok()?;
    }
    f.flush().ok()?;
    drop(f);
    std::fs::rename(&tmp, &path).ok()?;
    // tidy: keep the cache folder from growing without bound
    if let Ok(rd) = std::fs::read_dir(&dir) {
        let mut files: Vec<_> = rd.flatten().filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path(), e.metadata().ok()?.len()))).collect();
        files.sort();
        let mut total: u64 = files.iter().map(|f| f.2).sum();
        for (_, p, len) in files {
            if total < 1_500_000_000 || p == path {
                break;
            }
            if std::fs::remove_file(&p).is_ok() {
                total -= len;
            }
        }
    }
    Some(path)
}
