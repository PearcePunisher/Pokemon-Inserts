//! Minimal text rendering on top of ab_glyph: per-character font fallback,
//! tight-bounds centering and an optional outline stroke.

use ab_glyph::{point, Font, FontRef, Glyph, PxScale, ScaleFont};
use image::RgbaImage;
use std::sync::OnceLock;

static POKEMON: &[u8] = include_bytes!("../assets/pokemon_solid.ttf");
static SANS: &[u8] = include_bytes!("../assets/DejaVuSans-Bold.ttf");

pub struct Fonts {
    pub pokemon: FontRef<'static>,
    pub sans: FontRef<'static>,
}

pub fn fonts() -> &'static Fonts {
    static FONTS: OnceLock<Fonts> = OnceLock::new();
    FONTS.get_or_init(|| Fonts {
        pokemon: FontRef::try_from_slice(POKEMON).expect("bundled pokemon font"),
        sans: FontRef::try_from_slice(SANS).expect("bundled sans font"),
    })
}

/// A run of text laid out at a given pixel size, ready to draw.
pub struct Line {
    glyphs: Vec<(usize, Glyph)>,
    fonts: Vec<&'static FontRef<'static>>,
    /// Tight ink bounds relative to the pen origin (x0, y0, x1, y1).
    bounds: (f32, f32, f32, f32),
}

impl Line {
    /// Lays out `text`, taking each character from the first font that has it.
    pub fn new(text: &str, chain: &[&'static FontRef<'static>], px: f32) -> Line {
        let scale = PxScale::from(px);
        let mut glyphs = Vec::new();
        let mut x = 0.0f32;
        let mut prev: Option<(usize, ab_glyph::GlyphId)> = None;
        for c in text.chars() {
            let (fi, id) = chain
                .iter()
                .enumerate()
                .map(|(i, f)| (i, f.glyph_id(c)))
                .find(|(_, id)| id.0 != 0)
                .unwrap_or((chain.len() - 1, chain[chain.len() - 1].glyph_id(c)));
            let sf = chain[fi].as_scaled(scale);
            if let Some((pfi, pid)) = prev {
                if pfi == fi {
                    x += sf.kern(pid, id);
                }
            }
            glyphs.push((fi, id.with_scale_and_position(scale, point(x, 0.0))));
            x += sf.h_advance(id);
            prev = Some((fi, id));
        }

        let mut b = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for (fi, g) in &glyphs {
            if let Some(o) = chain[*fi].outline_glyph(g.clone()) {
                let r = o.px_bounds();
                b = (b.0.min(r.min.x), b.1.min(r.min.y), b.2.max(r.max.x), b.3.max(r.max.y));
            }
        }
        if b.0 > b.2 {
            b = (0.0, 0.0, 0.0, 0.0);
        }
        Line { glyphs, fonts: chain.to_vec(), bounds: b }
    }

    pub fn width(&self) -> f32 {
        self.bounds.2 - self.bounds.0
    }

    pub fn height(&self) -> f32 {
        self.bounds.3 - self.bounds.1
    }

    /// Draws the line with its ink box centered on (cx, cy).
    pub fn draw_centered(
        &self,
        img: &mut RgbaImage,
        cx: f32,
        cy: f32,
        fill: [u8; 3],
        stroke: Option<([u8; 3], f32)>,
    ) {
        let pad = stroke.map(|(_, w)| w.ceil() as i32 + 1).unwrap_or(1);
        let ox = (cx - self.width() / 2.0 - self.bounds.0).round() as i32;
        let oy = (cy - self.height() / 2.0 - self.bounds.1).round() as i32;
        let mx = self.bounds.0.floor() as i32 - pad;
        let my = self.bounds.1.floor() as i32 - pad;
        let mw = (self.width().ceil() as i32 + 2 * pad + 2) as usize;
        let mh = (self.height().ceil() as i32 + 2 * pad + 2) as usize;

        // Coverage mask of the fill.
        let mut mask = vec![0f32; mw * mh];
        for (fi, g) in &self.glyphs {
            if let Some(o) = self.fonts[*fi].outline_glyph(g.clone()) {
                let r = o.px_bounds();
                o.draw(|x, y, c| {
                    let px = r.min.x as i32 + x as i32 - mx;
                    let py = r.min.y as i32 + y as i32 - my;
                    if px >= 0 && py >= 0 && (px as usize) < mw && (py as usize) < mh {
                        let m = &mut mask[py as usize * mw + px as usize];
                        *m = (*m + c).min(1.0);
                    }
                });
            }
        }

        let blit = |img: &mut RgbaImage, m: &[f32], color: [u8; 3]| {
            for y in 0..mh {
                for x in 0..mw {
                    let a = m[y * mw + x];
                    if a <= 0.0 {
                        continue;
                    }
                    let (ix, iy) = (ox + mx + x as i32, oy + my + y as i32);
                    if ix < 0 || iy < 0 || ix >= img.width() as i32 || iy >= img.height() as i32 {
                        continue;
                    }
                    let p = img.get_pixel_mut(ix as u32, iy as u32);
                    for k in 0..3 {
                        p[k] = (p[k] as f32 * (1.0 - a) + color[k] as f32 * a).round() as u8;
                    }
                }
            }
        };

        if let Some((color, w)) = stroke {
            blit(img, &dilate(&mask, mw, mh, w), color);
        }
        blit(img, &mask, fill);
    }
}

/// Grows a coverage mask by `r` pixels (round brush) to make an outline.
fn dilate(mask: &[f32], w: usize, h: usize, r: f32) -> Vec<f32> {
    let ri = r.ceil() as i32;
    let offsets: Vec<(i32, i32, f32)> = (-ri..=ri)
        .flat_map(|dy| (-ri..=ri).map(move |dx| (dx, dy)))
        .filter_map(|(dx, dy)| {
            let d = ((dx * dx + dy * dy) as f32).sqrt();
            let weight = (r + 0.5 - d).clamp(0.0, 1.0);
            (weight > 0.0).then_some((dx, dy, weight))
        })
        .collect();
    let mut out = vec![0f32; w * h];
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let mut best = 0f32;
            for &(dx, dy, wt) in &offsets {
                let (sx, sy) = (x + dx, y + dy);
                if sx >= 0 && sy >= 0 && sx < w as i32 && sy < h as i32 {
                    best = best.max(mask[sy as usize * w + sx as usize] * wt);
                    if best >= 1.0 {
                        break;
                    }
                }
            }
            out[y as usize * w + x as usize] = best;
        }
    }
    out
}

/// Breaks `text` into at most `max_lines` lines that fit `max_width`,
/// shrinking the font from `px` down to `min_px` if needed.
pub fn fit_lines(
    text: &str,
    chain: &[&'static FontRef<'static>],
    mut px: f32,
    min_px: f32,
    max_width: f32,
    max_lines: usize,
) -> (Vec<Line>, f32) {
    loop {
        let words: Vec<&str> = text.split_whitespace().collect();
        let mut lines: Vec<String> = Vec::new();
        for w in &words {
            match lines.last_mut() {
                Some(cur) if Line::new(&format!("{cur} {w}"), chain, px).width() <= max_width => {
                    cur.push(' ');
                    cur.push_str(w);
                }
                _ => lines.push(w.to_string()),
            }
        }
        let laid: Vec<Line> = lines.iter().map(|l| Line::new(l, chain, px)).collect();
        let fits = laid.len() <= max_lines && laid.iter().all(|l| l.width() <= max_width);
        if fits || px <= min_px {
            return (laid, px);
        }
        px = (px * 0.92).max(min_px);
    }
}

pub const WHITE: [u8; 3] = [255, 255, 255];
pub const INK: [u8; 3] = [20, 20, 20];
