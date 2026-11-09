//! PNG and animated GIF encoding with nearest-neighbor upscaling, so pixel art stays sharp.

use std::borrow::Cow;
use std::collections::HashMap;

use crate::error::{Error, Result};
use crate::hud::{SHARE_CARD_HEIGHT, SHARE_CARD_WIDTH};
use crate::render::{Frame, FRAMES_PER_SECOND, FRAME_HEIGHT, FRAME_WIDTH};

/// Frames are exported at 2x (640x280).
pub const EXPORT_SCALE: u32 = 2;
/// The share card is exported at 4x (1280x640, the size of social previews).
pub const SHARE_CARD_SCALE: u32 = 4;

/// GIF palettes hold 256 entries; index 0 is reserved for transparency.
const GIF_MAX_COLORS: usize = 255;
const GIF_TRANSPARENT: u8 = 0;

pub fn frame_png(frame: &Frame) -> Result<Vec<u8>> {
    scaled_png(&frame.pixels, FRAME_WIDTH, FRAME_HEIGHT, EXPORT_SCALE)
}

pub fn share_card_png(pixels: &[[u8; 4]]) -> Result<Vec<u8>> {
    scaled_png(
        pixels,
        SHARE_CARD_WIDTH,
        SHARE_CARD_HEIGHT,
        SHARE_CARD_SCALE,
    )
}

fn scaled_png(pixels: &[[u8; 4]], w: u32, h: u32, scale: u32) -> Result<Vec<u8>> {
    let (out_w, out_h) = (w * scale, h * scale);
    let mut data = Vec::with_capacity((out_w * out_h * 4) as usize);
    for y in 0..out_h {
        let row = &pixels[((y / scale) * w) as usize..((y / scale + 1) * w) as usize];
        for px in row {
            for _ in 0..scale {
                data.extend_from_slice(px);
            }
        }
    }

    let encode_error = |e: png::EncodingError| Error::Encode(e.to_string());
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, out_w, out_h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(encode_error)?;
    writer.write_image_data(&data).map_err(encode_error)?;
    writer.finish().map_err(encode_error)?;
    Ok(bytes)
}

/// Encode the frames as an infinitely looping GIF. Each frame stores only the bounding box of
/// its non-transparent pixels with its own palette.
pub fn animated_gif(frames: &[Frame]) -> Result<Vec<u8>> {
    if frames.is_empty() {
        return Err(Error::Encode("a GIF needs at least one frame".to_owned()));
    }
    let encode_error = |e: gif::EncodingError| Error::Encode(e.to_string());
    let mut bytes = Vec::new();
    {
        let (w, h) = (
            (FRAME_WIDTH * EXPORT_SCALE) as u16,
            (FRAME_HEIGHT * EXPORT_SCALE) as u16,
        );
        let mut encoder = gif::Encoder::new(&mut bytes, w, h, &[]).map_err(encode_error)?;
        encoder
            .set_repeat(gif::Repeat::Infinite)
            .map_err(encode_error)?;
        for (i, frame) in frames.iter().enumerate() {
            encoder
                .write_frame(&gif_frame(frame, i))
                .map_err(encode_error)?;
        }
    }
    Ok(bytes)
}

fn gif_frame(frame: &Frame, index: usize) -> gif::Frame<'static> {
    let [left, top, width, height] = drawn_box(&frame.pixels);
    let mut palette_index: HashMap<[u8; 3], u8> = HashMap::new();
    let mut palette = vec![0_u8; 3];
    let mut indices = Vec::with_capacity((width * height * EXPORT_SCALE * EXPORT_SCALE) as usize);

    for y in top..top + height {
        let mut row = Vec::with_capacity((width * EXPORT_SCALE) as usize);
        for x in left..left + width {
            let [r, g, b, a] = frame.pixels[(y * FRAME_WIDTH + x) as usize];
            let idx = if a == 0 {
                GIF_TRANSPARENT
            } else if let Some(&idx) = palette_index.get(&[r, g, b]) {
                idx
            } else if palette_index.len() < GIF_MAX_COLORS {
                let idx = palette_index.len() as u8 + 1;
                palette_index.insert([r, g, b], idx);
                palette.extend([r, g, b]);
                idx
            } else {
                1
            };
            row.extend(std::iter::repeat_n(idx, EXPORT_SCALE as usize));
        }
        for _ in 0..EXPORT_SCALE {
            indices.extend_from_slice(&row);
        }
    }

    gif::Frame {
        delay: hundredths_at(index + 1) - hundredths_at(index),
        dispose: gif::DisposalMethod::Background,
        transparent: Some(GIF_TRANSPARENT),
        left: (left * EXPORT_SCALE) as u16,
        top: (top * EXPORT_SCALE) as u16,
        width: (width * EXPORT_SCALE) as u16,
        height: (height * EXPORT_SCALE) as u16,
        palette: Some(palette),
        buffer: Cow::Owned(indices),
        ..gif::Frame::default()
    }
}

/// `[left, top, width, height]` of the non-transparent pixels (a 1x1 box if there are none).
fn drawn_box(pixels: &[[u8; 4]]) -> [u32; 4] {
    let (mut left, mut top, mut right, mut bottom) = (FRAME_WIDTH, FRAME_HEIGHT, 0, 0);
    for (i, px) in pixels.iter().enumerate() {
        if px[3] != 0 {
            let (x, y) = (i as u32 % FRAME_WIDTH, i as u32 / FRAME_WIDTH);
            left = left.min(x);
            top = top.min(y);
            right = right.max(x);
            bottom = bottom.max(y);
        }
    }
    if left > right {
        [0, 0, 1, 1]
    } else {
        [left, top, right + 1 - left, bottom + 1 - top]
    }
}

/// Elapsed time at `frame` in GIF hundredths of a second, rounded so delays never drift.
fn hundredths_at(frame: usize) -> u16 {
    let fps = FRAMES_PER_SECOND as usize;
    ((frame * 200 + fps) / (2 * fps)) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_delays_add_up_to_the_loop_length() {
        let total: u32 = (0..48)
            .map(|i| u32::from(hundredths_at(i + 1) - hundredths_at(i)))
            .sum();
        assert_eq!(total, 320);
    }
}
