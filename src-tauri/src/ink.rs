//! Turns a photograph of a signature or a stamp on paper into ink on a
//! transparent background, ready to print over a signature line.
//!
//! A school will photograph the headteacher's signature or the stamp with a
//! phone: grey paper, a shadow across one corner, the ink blue or purple.
//! Printed as it is, that is a grey rectangle covering the line it should sit
//! on. Thresholding the brightness does not fix it either: the shadowed paper
//! is darker than the faint end of a pen stroke.
//!
//! So the paper is estimated locally instead of globally:
//!
//! 1. A *max filter* wipes out the ink, since a stroke is thinner than the
//!    window and paper is the brightest thing nearby.
//! 2. A *box blur* of that smooths it into a background that follows the
//!    shadow.
//! 3. A pixel is ink by how much darker it is than the paper behind it, and
//!    its colour is divided by that paper, so a shadow does not tint the ink.
//!
//! The result is cropped to the ink and scaled to a size that prints well.

use image::{imageops::FilterType, ImageFormat, ImageReader, Rgba, RgbaImage};
use std::io::Cursor;

use crate::error::{AppError, AppResult};

/// Largest file accepted.
pub const MAX_FILE_BYTES: usize = 8 * 1024 * 1024;
/// Work is done at this size; a phone photo is 4000 pixels wide.
const WORK_SIZE: u32 = 1400;
/// Finished size limits. A signature line is about 50 mm; 200 px high prints
/// sharply at that size.
const OUT_MAX_WIDTH: u32 = 800;
const OUT_MAX_HEIGHT: u32 = 260;
/// Darker than the paper by less than this is paper; by more than `INK_FULL`
/// is solid ink. On a 0-255 scale.
const INK_START: f32 = 22.0;
const INK_FULL: f32 = 70.0;

/// Which kind of mark, which changes only how much ink is plausible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Signature,
    Stamp,
}

/// Decodes a PNG or JPEG and returns the cleaned mark as a PNG.
pub fn clean(bytes: &[u8], mark: Mark) -> AppResult<Vec<u8>> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(AppError::validation("That image is larger than 8 MB. Please use a smaller photo."));
    }
    let format = match image::guess_format(bytes) {
        Ok(ImageFormat::Png) => ImageFormat::Png,
        Ok(ImageFormat::Jpeg) => ImageFormat::Jpeg,
        _ => {
            return Err(AppError::validation(
                "That file is not a photo Phantom can read. Use a JPEG or PNG.",
            ))
        }
    };

    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(12_000);
    limits.max_image_height = Some(12_000);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|_| AppError::validation("That image could not be read. Try taking the photo again."))?;

    let rgb = decoded.thumbnail(WORK_SIZE, WORK_SIZE).to_rgba8();
    let cleaned = extract(&rgb, mark)?;
    encode_png(&cleaned)
}

/// Checks a PNG handed back by the interface before it is stored: it must
/// decode, and be no bigger than this module ever produces.
pub fn validate_png(bytes: &[u8]) -> AppResult<()> {
    if bytes.len() > 2 * 1024 * 1024 || !matches!(image::guess_format(bytes), Ok(ImageFormat::Png)) {
        return Err(AppError::validation("That is not a prepared signature image."));
    }
    let img = image::load_from_memory_with_format(bytes, ImageFormat::Png)
        .map_err(|_| AppError::validation("That is not a prepared signature image."))?;
    if img.width() > OUT_MAX_WIDTH || img.height() > OUT_MAX_HEIGHT.max(OUT_MAX_WIDTH) {
        return Err(AppError::validation("That signature image is too large."));
    }
    Ok(())
}

fn encode_png(img: &RgbaImage) -> AppResult<Vec<u8>> {
    let mut out = Vec::new();
    img.write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .map_err(|e| AppError::internal(format!("could not encode the signature: {e}")))?;
    Ok(out)
}

fn luma(p: &Rgba<u8>) -> f32 {
    0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32
}

/// Sliding-window maximum along rows, then columns: the brightest value
/// within `r` pixels. Wipes out anything thinner than the window.
fn max_filter(src: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    let mut tmp = vec![0.0f32; w * h];
    for y in 0..h {
        let row = &src[y * w..(y + 1) * w];
        for x in 0..w {
            let lo = x.saturating_sub(r);
            let hi = (x + r).min(w - 1);
            tmp[y * w + x] = row[lo..=hi].iter().cloned().fold(f32::MIN, f32::max);
        }
    }
    let mut out = vec![0.0f32; w * h];
    for x in 0..w {
        for y in 0..h {
            let lo = y.saturating_sub(r);
            let hi = (y + r).min(h - 1);
            let mut m = f32::MIN;
            for yy in lo..=hi {
                m = m.max(tmp[yy * w + x]);
            }
            out[y * w + x] = m;
        }
    }
    out
}

/// Mean over a (2r+1)² window, via a summed-area table.
fn box_blur(src: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    let mut sat = vec![0.0f64; (w + 1) * (h + 1)];
    for y in 0..h {
        let mut row = 0.0f64;
        for x in 0..w {
            row += src[y * w + x] as f64;
            sat[(y + 1) * (w + 1) + (x + 1)] = sat[y * (w + 1) + (x + 1)] + row;
        }
    }
    let mut out = vec![0.0f32; w * h];
    for y in 0..h {
        let (y0, y1) = (y.saturating_sub(r), (y + r + 1).min(h));
        for x in 0..w {
            let (x0, x1) = (x.saturating_sub(r), (x + r + 1).min(w));
            let sum = sat[y1 * (w + 1) + x1] - sat[y0 * (w + 1) + x1] - sat[y1 * (w + 1) + x0]
                + sat[y0 * (w + 1) + x0];
            out[y * w + x] = (sum / ((y1 - y0) * (x1 - x0)) as f64) as f32;
        }
    }
    out
}

fn extract(img: &RgbaImage, mark: Mark) -> AppResult<RgbaImage> {
    let (w, h) = (img.width() as usize, img.height() as usize);
    if w < 16 || h < 16 {
        return Err(AppError::validation("That image is too small to hold a signature."));
    }
    let lum: Vec<f32> = img.pixels().map(luma).collect();

    // The ink-erasing window must be wider than a stroke but much narrower
    // than the shadow it has to follow.
    let side = w.max(h);
    let erase = (side / 60).clamp(3, 40);
    let smooth = (side / 25).clamp(6, 80);
    let paper = box_blur(&max_filter(&lum, w, h, erase), w, h, smooth);

    let mut out = RgbaImage::new(w as u32, h as u32);
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (usize::MAX, usize::MAX, 0usize, 0usize);
    let mut inked = 0usize;
    let mut ink_rgb = [0f64; 3];
    let mut ink_weight = 0f64;

    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let bg = paper[i].max(lum[i]).max(1.0);
            let darker = bg - lum[i];
            let a = ((darker - INK_START) / (INK_FULL - INK_START)).clamp(0.0, 1.0);
            let p = img.get_pixel(x as u32, y as u32);
            // Divide by the paper so a shadow does not tint the ink.
            let scale = 255.0 / bg;
            let rgb = [
                (p[0] as f32 * scale).min(255.0),
                (p[1] as f32 * scale).min(255.0),
                (p[2] as f32 * scale).min(255.0),
            ];
            let alpha = (a * 255.0).round() as u8;
            out.put_pixel(x as u32, y as u32, Rgba([rgb[0] as u8, rgb[1] as u8, rgb[2] as u8, alpha]));
            if alpha > 96 {
                inked += 1;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
                let weight = a as f64;
                for c in 0..3 {
                    ink_rgb[c] += rgb[c] as f64 * weight;
                }
                ink_weight += weight;
            }
        }
    }

    let share = inked as f64 / (w * h) as f64;
    if inked < 40 {
        return Err(AppError::validation(
            "No ink was found. Photograph the signature on plain white paper, in good light.",
        ));
    }
    let too_much = match mark {
        Mark::Signature => 0.35,
        Mark::Stamp => 0.6,
    };
    if share > too_much {
        return Err(AppError::validation(
            "This looks like a photo of something other than ink on white paper. Photograph just the signature or stamp, filling most of the frame with paper.",
        ));
    }

    // Transparent pixels take the ink's colour, so scaling does not leave a
    // white halo round each stroke.
    let ink = [
        (ink_rgb[0] / ink_weight) as u8,
        (ink_rgb[1] / ink_weight) as u8,
        (ink_rgb[2] / ink_weight) as u8,
    ];
    for p in out.pixels_mut() {
        if p[3] < 24 {
            *p = Rgba([ink[0], ink[1], ink[2], 0]);
        }
    }

    let pad = ((max_x - min_x).max(max_y - min_y) / 25).max(4);
    let x0 = min_x.saturating_sub(pad) as u32;
    let y0 = min_y.saturating_sub(pad) as u32;
    let x1 = ((max_x + pad).min(w - 1) + 1) as u32;
    let y1 = ((max_y + pad).min(h - 1) + 1) as u32;
    let cropped = image::imageops::crop_imm(&out, x0, y0, x1 - x0, y1 - y0).to_image();

    let (max_w, max_h) = match mark {
        Mark::Signature => (OUT_MAX_WIDTH, OUT_MAX_HEIGHT),
        Mark::Stamp => (OUT_MAX_HEIGHT * 2, OUT_MAX_HEIGHT * 2),
    };
    let fits = cropped.width() <= max_w && cropped.height() <= max_h;
    Ok(if fits {
        cropped
    } else {
        let ratio = (max_w as f32 / cropped.width() as f32).min(max_h as f32 / cropped.height() as f32);
        image::imageops::resize(
            &cropped,
            ((cropped.width() as f32 * ratio).round() as u32).max(1),
            ((cropped.height() as f32 * ratio).round() as u32).max(1),
            FilterType::Lanczos3,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic phone photo: paper that darkens from left to right as if a
    /// hand were shadowing it, with a blue pen stroke across the middle.
    fn photo(w: u32, h: u32, shadow: f32) -> RgbaImage {
        let mut img = RgbaImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let paper = 235.0 - shadow * x as f32 / w as f32;
                img.put_pixel(x, y, Rgba([paper as u8, paper as u8, (paper - 6.0) as u8, 255]));
            }
        }
        // A wavy stroke, four pixels thick.
        for x in w / 6..w * 5 / 6 {
            let yc = h as f32 / 2.0 + (x as f32 / 18.0).sin() * h as f32 / 6.0;
            for dy in -2..=2 {
                let y = (yc as i32 + dy) as u32;
                img.put_pixel(x, y, Rgba([30, 45, 140, 255]));
            }
        }
        img
    }

    fn png(img: &RgbaImage) -> Vec<u8> {
        encode_png(img).unwrap()
    }

    #[test]
    fn shadowed_paper_becomes_transparent_and_the_stroke_stays() {
        // The shadowed right edge (paper at 235-110=125) is darker than the
        // stroke's own brightness would suggest a threshold for.
        let out = extract(&photo(600, 240, 110.0), Mark::Signature).unwrap();
        let opaque = out.pixels().filter(|p| p[3] > 200).count();
        let clear = out.pixels().filter(|p| p[3] == 0).count();
        assert!(opaque > 1000, "the stroke survives ({opaque} opaque pixels)");
        assert!(clear > out.pixels().count() / 2, "the paper is gone");
    }

    #[test]
    fn the_ink_keeps_its_colour() {
        let out = extract(&photo(600, 240, 60.0), Mark::Signature).unwrap();
        let ink: Vec<&Rgba<u8>> = out.pixels().filter(|p| p[3] > 230).collect();
        let blue = ink.iter().filter(|p| p[2] > p[0] + 40).count();
        assert!(blue * 10 > ink.len() * 9, "blue ink prints blue");
    }

    #[test]
    fn the_result_is_cropped_to_the_ink() {
        let out = extract(&photo(1200, 900, 40.0), Mark::Signature).unwrap();
        // The stroke spans 2/3 of the width and about 1/3 of the height.
        assert!(out.width() as f32 / out.height() as f32 > 1.6, "{}x{}", out.width(), out.height());
        assert!(out.width() <= OUT_MAX_WIDTH && out.height() <= OUT_MAX_HEIGHT);
    }

    #[test]
    fn blank_paper_is_refused() {
        // Shadowed paper with nothing written on it.
        let mut paper = RgbaImage::new(400, 200);
        for (x, _, p) in paper.enumerate_pixels_mut() {
            let v = (235.0 - 80.0 * x as f32 / 400.0) as u8;
            *p = Rgba([v, v, v, 255]);
        }
        assert!(matches!(extract(&paper, Mark::Signature), Err(AppError::Validation(_))));
    }

    #[test]
    fn a_dark_photo_is_not_mistaken_for_a_signature() {
        let mut img = RgbaImage::new(300, 300);
        for (x, y, p) in img.enumerate_pixels_mut() {
            let v = if (x / 7 + y / 11) % 2 == 0 { 40 } else { 200 };
            *p = Rgba([v, v, v, 255]);
        }
        assert!(extract(&img, Mark::Signature).is_err());
    }

    #[test]
    fn jpeg_and_png_both_decode_and_garbage_does_not() {
        let img = photo(500, 200, 50.0);
        let mut jpeg = Vec::new();
        image::DynamicImage::ImageRgba8(img.clone())
            .to_rgb8()
            .write_to(&mut Cursor::new(&mut jpeg), ImageFormat::Jpeg)
            .unwrap();
        assert!(clean(&jpeg, Mark::Signature).is_ok());
        let cleaned = clean(&png(&img), Mark::Signature).unwrap();
        assert!(validate_png(&cleaned).is_ok());
        assert!(clean(b"GIF89a not really", Mark::Signature).is_err());
        assert!(clean(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 1, 2, 3], Mark::Signature).is_err());
    }
}
