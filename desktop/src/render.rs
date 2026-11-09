//! Draws a single insert. Everything is expressed at 300 DPI (750x1050 px for
//! a 2.5" x 3.5" card) and multiplied by `scale`, so the live preview can
//! render the exact same design at a smaller size.

use crate::scrape::Card;
use crate::text::{fit_lines, fonts, Line, INK, WHITE};
use image::imageops::{self, FilterType};
use image::{DynamicImage, Rgba, RgbaImage, RgbImage};

pub const CARD_W: u32 = 750;
pub const CARD_H: u32 = 1050;
const CORNER_RADIUS: f32 = 40.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Style {
    /// Blurred card art with the number on top.
    Art,
    /// White insert with an outline, number and name only.
    PrinterFriendly,
}

#[derive(Clone, PartialEq, Debug)]
pub struct RenderSettings {
    pub style: Style,
    /// Blur strength (Gaussian sigma at 300 DPI). Art style only.
    pub blur: f32,
    pub show_name: bool,
}

/// Renders an insert. `art` may be `None` for the printer-friendly style.
/// Returns RGBA where alpha follows the rounded card shape.
pub fn render(
    card: &Card,
    set_code: &str,
    art: Option<&DynamicImage>,
    s: &RenderSettings,
    scale: f32,
    fast: bool,
) -> RgbaImage {
    let w = (CARD_W as f32 * scale).round() as u32;
    let h = (CARD_H as f32 * scale).round() as u32;
    let f = fonts();
    let number = card.display_number();

    let mut img = match (s.style, art) {
        (Style::Art, Some(art)) => {
            let filter = if fast { FilterType::Triangle } else { FilterType::Lanczos3 };
            let base = art.resize_exact(w, h, filter).to_rgba8();
            if s.blur > 0.05 {
                imageops::fast_blur(&base, s.blur * scale)
            } else {
                base
            }
        }
        _ => RgbaImage::from_pixel(w, h, Rgba([255, 255, 255, 255])),
    };

    let (wf, hf) = (w as f32, h as f32);
    match s.style {
        Style::Art => {
            let num = Line::new(&number, &[&f.pokemon, &f.sans], 160.0 * scale);
            let num_y = if s.show_name { hf * 0.42 } else { hf * 0.5 };
            num.draw_centered(&mut img, wf / 2.0, num_y, WHITE, Some((INK, 5.0 * scale)));
            if s.show_name {
                draw_name(&mut img, &card.name, hf * 0.62, scale, WHITE, Some((INK, 3.5 * scale)));
            }
        }
        Style::PrinterFriendly => {
            let num = Line::new(&number, &[&f.pokemon, &f.sans], 190.0 * scale);
            let num_y = if s.show_name { hf * 0.38 } else { hf * 0.5 };
            num.draw_centered(&mut img, wf / 2.0, num_y, INK, None);
            if s.show_name {
                draw_name(&mut img, &card.name, hf * 0.62, scale, INK, None);
            }
            let code = Line::new(set_code, &[&f.sans], 32.0 * scale);
            code.draw_centered(&mut img, wf / 2.0, hf - 75.0 * scale, [130, 130, 130], None);
        }
    }

    apply_shape(&mut img, scale, s.style == Style::PrinterFriendly);
    img
}

fn draw_name(
    img: &mut RgbaImage,
    name: &str,
    cy: f32,
    scale: f32,
    fill: [u8; 3],
    stroke: Option<([u8; 3], f32)>,
) {
    let f = fonts();
    let max_w = img.width() as f32 * 0.82;
    let (lines, px) = fit_lines(name, &[&f.sans], 70.0 * scale, 34.0 * scale, max_w, 3);
    let spacing = px * 1.2;
    let n = lines.len() as f32;
    for (i, line) in lines.iter().enumerate() {
        let y = cy + (i as f32 - (n - 1.0) / 2.0) * spacing;
        line.draw_centered(img, img.width() as f32 / 2.0, y, fill, stroke);
    }
}

/// Rounds the corners (via alpha) and optionally draws the cut outline.
fn apply_shape(img: &mut RgbaImage, scale: f32, outline: bool) {
    let (w, h) = (img.width() as f32, img.height() as f32);
    let r = CORNER_RADIUS * scale;
    let stroke = (5.0 * scale).max(1.0);
    let outline_color = [70u8, 70, 70];
    for (x, y, p) in img.enumerate_pixels_mut() {
        let d = rounded_rect_sdf(x as f32 + 0.5, y as f32 + 0.5, w, h, r);
        let outer = (0.5 - d).clamp(0.0, 1.0);
        if outline {
            let ring = outer - (0.5 - (d + stroke)).clamp(0.0, 1.0);
            for k in 0..3 {
                p[k] = (p[k] as f32 * (1.0 - ring) + outline_color[k] as f32 * ring).round() as u8;
            }
        }
        p[3] = (outer * 255.0).round() as u8;
    }
}

/// Signed distance from a point to the edge of a w x h rounded rectangle.
fn rounded_rect_sdf(x: f32, y: f32, w: f32, h: f32, r: f32) -> f32 {
    let qx = (x - w / 2.0).abs() - (w / 2.0 - r);
    let qy = (y - h / 2.0).abs() - (h / 2.0 - r);
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    outside + qx.max(qy).min(0.0) - r
}

/// Flattens onto white paper for printing.
pub fn on_white(img: &RgbaImage) -> RgbImage {
    RgbImage::from_fn(img.width(), img.height(), |x, y| {
        let p = img.get_pixel(x, y);
        let a = p[3] as f32 / 255.0;
        let mix = |c: u8| (c as f32 * a + 255.0 * (1.0 - a)).round() as u8;
        image::Rgb([mix(p[0]), mix(p[1]), mix(p[2])])
    })
}
