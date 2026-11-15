//! Audio synchronisation for multicam: find how far two recordings of the same event are offset
//! by cross-correlating their sound (FFT). No UI or egui dependencies.
use crate::util::command;
use std::io::Read;
use std::path::Path;
use std::process::Stdio;

pub const RATE: f64 = 4000.0;
/// Seconds of audio analysed from each file.
pub const ANALYSE_SECS: f64 = 90.0;

fn fft(re: &mut [f64], im: &mut [f64], inverse: bool) {
    let n = re.len();
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = 2.0 * std::f64::consts::PI / len as f64 * if inverse { -1.0 } else { 1.0 };
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f64, 0.0f64);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let (tr, ti) = (re[b] * cr - im[b] * ci, re[b] * ci + im[b] * cr);
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let nr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = nr;
            }
        }
        len <<= 1;
    }
    if inverse {
        for i in 0..n {
            re[i] /= n as f64;
            im[i] /= n as f64;
        }
    }
}

/// Emphasise transients (so steady hum does not dominate) and remove the mean.
fn prepare(x: &[f32]) -> Vec<f64> {
    let mut d: Vec<f64> = (0..x.len()).map(|i| if i == 0 { 0.0 } else { (x[i] - x[i - 1]) as f64 }).collect();
    let mean = d.iter().sum::<f64>() / d.len().max(1) as f64;
    d.iter_mut().for_each(|v| *v -= mean);
    d
}

/// Lag in seconds such that `a[t] ~ b[t - lag]` (positive: the event happens later in `a`),
/// and a confidence ratio (peak height over the average correlation; above ~6 is reliable).
pub fn lag_seconds(a: &[f32], b: &[f32], rate: f64) -> Option<(f64, f64)> {
    if a.len() < 256 || b.len() < 256 {
        return None;
    }
    let (pa, pb) = (prepare(a), prepare(b));
    let n = (pa.len() + pb.len()).next_power_of_two();
    let (mut ar, mut ai) = (vec![0.0; n], vec![0.0; n]);
    let (mut br, mut bi) = (vec![0.0; n], vec![0.0; n]);
    ar[..pa.len()].copy_from_slice(&pa);
    br[..pb.len()].copy_from_slice(&pb);
    fft(&mut ar, &mut ai, false);
    fft(&mut br, &mut bi, false);
    // A * conj(B)
    for i in 0..n {
        let (r, im) = (ar[i] * br[i] + ai[i] * bi[i], ai[i] * br[i] - ar[i] * bi[i]);
        ar[i] = r;
        ai[i] = im;
    }
    fft(&mut ar, &mut ai, true);
    let (lo, hi) = (-(pb.len() as i64 - 1), pa.len() as i64 - 1);
    let (mut best, mut best_k, mut sum, mut cnt) = (f64::MIN, 0i64, 0.0, 0usize);
    for k in lo..=hi {
        let v = ar[if k >= 0 { k as usize } else { (n as i64 + k) as usize }];
        sum += v.abs();
        cnt += 1;
        if v > best {
            best = v;
            best_k = k;
        }
    }
    let mean = sum / cnt.max(1) as f64;
    Some((best_k as f64 / rate, if mean > 0.0 { best / mean } else { 0.0 }))
}

/// Decode up to `secs` seconds of mono audio at `RATE` from `from`.
pub fn load_audio(path: &Path, from: f64, secs: f64) -> Result<Vec<f32>, String> {
    let mut child = command("ffmpeg")
        .args(["-v", "error", "-nostdin", "-ss", &format!("{:.3}", from.max(0.0)), "-t", &format!("{secs:.3}"), "-i"])
        .arg(path)
        .args(["-vn", "-ac", "1", "-ar", &format!("{}", RATE as u32), "-f", "f32le", "-"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Could not start ffmpeg: {e}"))?;
    let mut bytes = vec![];
    child.stdout.take().ok_or("no output")?.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    let _ = child.wait();
    let v: Vec<f32> = bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
    if v.is_empty() {
        Err("No audio found.".into())
    } else {
        Ok(v)
    }
}

/// Offset between two media files: positive means an event happens later in `a` than in `b`
/// (so `b` must start that much later on the timeline). Also returns the confidence.
pub fn media_offset(a: &Path, b: &Path) -> Result<(f64, f64), String> {
    let (xa, xb) = (load_audio(a, 0.0, ANALYSE_SECS)?, load_audio(b, 0.0, ANALYSE_SECS)?);
    lag_seconds(&xa, &xb, RATE).ok_or_else(|| "Not enough audio to compare.".to_string())
}
