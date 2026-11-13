//! Video scopes computed from an RGBA frame: waveform, RGB parade, vectorscope, histogram.
//! Each returns an RGBA image (w * h * 4) ready to upload as a texture.

pub struct ScopeImage {
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
}

fn blank(w: usize, h: usize) -> Vec<u8> {
    let mut v = vec![0u8; w * h * 4];
    for px in v.chunks_exact_mut(4) {
        px.copy_from_slice(&[12, 13, 15, 255]);
    }
    v
}

fn put_add(img: &mut [u8], w: usize, x: usize, y: usize, c: [u8; 3], amt: u8) {
    let i = (y * w + x) * 4;
    for k in 0..3 {
        img[i + k] = img[i + k].saturating_add(((c[k] as u16 * amt as u16) / 255) as u8);
    }
}

fn graticule(img: &mut [u8], w: usize, h: usize) {
    for k in 0..=4 {
        let y = ((h - 1) * k) / 4;
        for x in 0..w {
            let i = (y * w + x) * 4;
            for c in 0..3 {
                img[i + c] = img[i + c].max(48);
            }
        }
    }
}

fn luma(p: &[u8]) -> f32 {
    0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32
}

/// Luma waveform: x = picture column, y = brightness (top = 255).
pub fn waveform(frame: &[u8], fw: usize, fh: usize, w: usize, h: usize) -> ScopeImage {
    let mut img = blank(w, h);
    if fw > 0 && fh > 0 {
        let step = (fh / 120).max(1);
        for y in (0..fh).step_by(step) {
            for x in 0..fw {
                let p = &frame[(y * fw + x) * 4..];
                let l = luma(p);
                let ox = x * w / fw;
                let oy = h - 1 - ((l / 255.0) * (h - 1) as f32) as usize;
                put_add(&mut img, w, ox, oy.min(h - 1), [90, 255, 120], 26);
            }
        }
    }
    graticule(&mut img, w, h);
    ScopeImage { w, h, rgba: img }
}

/// RGB parade: three side-by-side waveforms.
pub fn parade(frame: &[u8], fw: usize, fh: usize, w: usize, h: usize) -> ScopeImage {
    let mut img = blank(w, h);
    let third = (w / 3).max(1);
    if fw > 0 && fh > 0 {
        let step = (fh / 120).max(1);
        let cols = [[255u8, 60, 60], [60, 255, 60], [80, 120, 255]];
        for y in (0..fh).step_by(step) {
            for x in 0..fw {
                let p = &frame[(y * fw + x) * 4..];
                let ox = x * third / fw;
                for ch in 0..3 {
                    let oy = h - 1 - ((p[ch] as f32 / 255.0) * (h - 1) as f32) as usize;
                    put_add(&mut img, w, (ch * third + ox).min(w - 1), oy.min(h - 1), cols[ch], 30);
                }
            }
        }
    }
    graticule(&mut img, w, h);
    ScopeImage { w, h, rgba: img }
}

/// Vectorscope: chroma (Cb horizontal, Cr vertical) on a square plot.
pub fn vectorscope(frame: &[u8], fw: usize, fh: usize, size: usize) -> ScopeImage {
    let mut img = blank(size, size);
    let c = size as f32 / 2.0;
    // circle graticule
    for a in 0..720 {
        let t = a as f32 * std::f32::consts::PI / 360.0;
        for r in [0.5f32, 0.95] {
            let x = (c + t.cos() * c * r) as usize;
            let y = (c + t.sin() * c * r) as usize;
            if x < size && y < size {
                put_add(&mut img, size, x, y, [60, 60, 60], 255);
            }
        }
    }
    if fw > 0 && fh > 0 {
        let step = ((fw * fh) / 40_000).max(1);
        let mut i = 0;
        while i < fw * fh {
            let p = &frame[i * 4..];
            let (r, g, b) = (p[0] as f32, p[1] as f32, p[2] as f32);
            let cb = -0.1146 * r - 0.3854 * g + 0.5 * b; // -127..127
            let cr = 0.5 * r - 0.4542 * g - 0.0458 * b;
            let x = (c + cb / 128.0 * c * 0.95).clamp(0.0, size as f32 - 1.0) as usize;
            let y = (c - cr / 128.0 * c * 0.95).clamp(0.0, size as f32 - 1.0) as usize;
            put_add(&mut img, size, x, y, [r as u8 / 2 + 100, g as u8 / 2 + 100, b as u8 / 2 + 100], 90);
            i += step;
        }
    }
    ScopeImage { w: size, h: size, rgba: img }
}

/// RGB histogram.
pub fn histogram(frame: &[u8], fw: usize, fh: usize, w: usize, h: usize) -> ScopeImage {
    let mut bins = [[0u32; 256]; 3];
    for i in 0..fw * fh {
        let p = &frame[i * 4..];
        for ch in 0..3 {
            bins[ch][p[ch] as usize] += 1;
        }
    }
    let max = bins.iter().flat_map(|b| b.iter()).copied().max().unwrap_or(1).max(1) as f32;
    let mut img = blank(w, h);
    let cols = [[255u8, 70, 70], [70, 255, 70], [90, 130, 255]];
    for x in 0..w {
        let b = x * 256 / w;
        for ch in 0..3 {
            let v = ((bins[ch][b] as f32 / max).sqrt() * (h - 1) as f32) as usize;
            for y in 0..v {
                put_add(&mut img, w, x, h - 1 - y, cols[ch], 110);
            }
        }
    }
    ScopeImage { w, h, rgba: img }
}
