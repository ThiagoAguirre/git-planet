//! Pixel-font HUD drawn over each frame, and the 1280x640 share card.

use std::cmp::Ordering;

use crate::palette::{parse_hex_color, Palette};
use crate::planet::PlanetBiology;
use crate::render::{Frame, FRAME_HEIGHT, FRAME_WIDTH, OUTLINE_COLOR};
use crate::snapshot::AccountSnapshot;

pub const SHARE_CARD_WIDTH: u32 = 320;
pub const SHARE_CARD_HEIGHT: u32 = 160;

/// Left edge of the HUD and its first line's distance from the frame bottom (inside the panel).
const HUD_X: u32 = 12;
const HUD_TOP_FROM_BOTTOM: u32 = 27;
const LINE_SPACING: u32 = 8;
const GLYPH_ADVANCE: u32 = 4;
const MAX_LANGUAGES: usize = 3;
const LANGUAGE_NAME_CHARS: usize = 8;

const STAGE_PILLS: [&str; 5] = ["1.PROTO", "2.RED", "3.MARIA", "4.ATMOS", "5.MARS"];
const CARD_FRAME_Y: u32 = 14;
const CARD_PILLS_X: u32 = 148;
const CARD_BACKGROUND: [u8; 4] = [13, 17, 23, 255];
const CARD_HEADER: [u8; 4] = [19, 25, 34, 255];
const FUTURE_STAGE_GRAY: [u8; 4] = [70, 78, 90, 255];

/// Bottom-left HUD: stage, contribution count, and top languages with color pips.
pub fn paint_metrics(
    pixels: &mut [[u8; 4]],
    bio: &PlanetBiology,
    palette: &Palette,
    snapshot: &AccountSnapshot,
) {
    let canvas = Canvas::new(pixels, FRAME_WIDTH, FRAME_HEIGHT);
    let line1 = FRAME_HEIGHT - HUD_TOP_FROM_BOTTOM;
    let line2 = line1 + LINE_SPACING;
    let line3 = line2 + LINE_SPACING;
    let mut canvas = canvas;

    canvas.fill_rect(HUD_X, line1, 3, 5, palette.hud_accent);
    canvas.text(HUD_X + 6, line1, bio.hud_stage_label(), palette.hud_text);
    let count = format!("CONTRIBUTIONS: {}", bio.commits);
    canvas.text(HUD_X, line2, &count, palette.hud_accent);

    let mut x = HUD_X;
    for language in snapshot.languages.iter().take(MAX_LANGUAGES) {
        if let Some([r, g, b]) = parse_hex_color(&language.hex_color) {
            canvas.fill_rect(x, line3 + 1, 3, 3, [r, g, b, 255]);
        }
        x += 5;
        let name: String = language.name.chars().take(LANGUAGE_NAME_CHARS).collect();
        canvas.text(x, line3, &name, palette.hud_dim);
        x += name.chars().count() as u32 * GLYPH_ADVANCE + 6;
    }
}

/// A 320x160 card (exported at 4x) with a header, the still frame, and a stage progress row.
pub fn render_share_card(bio: &PlanetBiology, palette: &Palette, still: &Frame) -> Vec<[u8; 4]> {
    let (w, h) = (SHARE_CARD_WIDTH, SHARE_CARD_HEIGHT);
    let mut pixels = vec![CARD_BACKGROUND; (w * h) as usize];
    let border = palette.soil[2];

    for x in 3..w - 3 {
        pixels[(3 * w + x) as usize] = border;
        pixels[((h - 4) * w + x) as usize] = border;
    }
    for y in 3..h - 3 {
        pixels[(y * w + 3) as usize] = border;
        pixels[(y * w + w - 4) as usize] = border;
    }
    for y in 4..18 {
        for x in 4..w - 4 {
            pixels[(y * w + x) as usize] = CARD_HEADER;
        }
    }
    for x in 4..w - 4 {
        pixels[(18 * w + x) as usize] = border;
    }

    for sy in 0..FRAME_HEIGHT {
        let dy = sy + CARD_FRAME_Y;
        if dy >= h - 4 {
            break;
        }
        for sx in 4..FRAME_WIDTH - 4 {
            let px = still.pixels[(sy * FRAME_WIDTH + sx) as usize];
            if px[3] != 0 {
                pixels[(dy * w + sx) as usize] = px;
            }
        }
    }

    let mut canvas = Canvas::new(&mut pixels, w, h);
    canvas.text(9, 9, "GIT-PLANET", palette.hud_text);
    let label = bio.hud_stage_label();
    let label_x = w.saturating_sub(label.len() as u32 * GLYPH_ADVANCE + 8);
    canvas.text(label_x, 9, label, palette.hud_accent);

    // Same row as the HUD's language list.
    let pills_y = CARD_FRAME_Y + FRAME_HEIGHT - HUD_TOP_FROM_BOTTOM + 2 * LINE_SPACING;
    let active = bio.stage_index();
    let mut x = CARD_PILLS_X;
    for (idx, pill) in STAGE_PILLS.iter().enumerate() {
        let color = match idx.cmp(&active) {
            Ordering::Equal => palette.hud_text,
            Ordering::Less => palette.hud_accent,
            Ordering::Greater => FUTURE_STAGE_GRAY,
        };
        canvas.text(x, pills_y, pill, color);
        x += pill.len() as u32 * GLYPH_ADVANCE + 6;
    }
    pixels
}

/// A mutable view over an RGBA buffer for drawing outlined pixel text.
struct Canvas<'a> {
    pixels: &'a mut [[u8; 4]],
    w: u32,
    h: u32,
}

impl<'a> Canvas<'a> {
    fn new(pixels: &'a mut [[u8; 4]], w: u32, h: u32) -> Self {
        Self { pixels, w, h }
    }

    fn fill_rect(&mut self, x: u32, y: u32, w: u32, h: u32, color: [u8; 4]) {
        for dy in 0..h {
            for dx in 0..w {
                self.outlined_pixel(x + dx, y + dy, color);
            }
        }
    }

    fn text(&mut self, x: u32, y: u32, text: &str, color: [u8; 4]) {
        for (i, ch) in text.chars().enumerate() {
            let left = x + i as u32 * GLYPH_ADVANCE;
            for (row, bits) in glyph_3x5(ch).iter().enumerate() {
                for col in 0..3_u32 {
                    if (bits >> (2 - col)) & 1 == 1 {
                        self.outlined_pixel(left + col, y + row as u32, color);
                    }
                }
            }
        }
    }

    /// Paint a pixel and give it a dark 1px outline wherever the canvas is transparent.
    fn outlined_pixel(&mut self, x: u32, y: u32, color: [u8; 4]) {
        if x == 0 || y == 0 || x >= self.w - 1 || y >= self.h - 1 {
            return;
        }
        for ny in y - 1..=y + 1 {
            for nx in x - 1..=x + 1 {
                let i = (ny * self.w + nx) as usize;
                if self.pixels[i][3] == 0 {
                    self.pixels[i] = OUTLINE_COLOR;
                }
            }
        }
        self.pixels[(y * self.w + x) as usize] = color;
    }
}

/// 3x5 pixel-art bitmap font for uppercase letters, digits, and punctuation.
fn glyph_3x5(ch: char) -> [u8; 5] {
    match ch.to_ascii_uppercase() {
        'A' => [0b010, 0b101, 0b111, 0b101, 0b101],
        'B' => [0b110, 0b101, 0b110, 0b101, 0b110],
        'C' => [0b011, 0b100, 0b100, 0b100, 0b011],
        'D' => [0b110, 0b101, 0b101, 0b101, 0b110],
        'E' => [0b111, 0b100, 0b110, 0b100, 0b111],
        'F' => [0b111, 0b100, 0b110, 0b100, 0b100],
        'G' => [0b011, 0b100, 0b101, 0b101, 0b011],
        'H' => [0b101, 0b101, 0b111, 0b101, 0b101],
        'I' => [0b111, 0b010, 0b010, 0b010, 0b111],
        'J' => [0b001, 0b001, 0b001, 0b101, 0b010],
        'K' => [0b101, 0b101, 0b110, 0b101, 0b101],
        'L' => [0b100, 0b100, 0b100, 0b100, 0b111],
        'M' => [0b101, 0b111, 0b111, 0b101, 0b101],
        'N' => [0b110, 0b101, 0b101, 0b101, 0b101],
        'O' => [0b010, 0b101, 0b101, 0b101, 0b010],
        'P' => [0b110, 0b101, 0b110, 0b100, 0b100],
        'Q' => [0b010, 0b101, 0b101, 0b011, 0b001],
        'R' => [0b110, 0b101, 0b110, 0b101, 0b101],
        'S' => [0b011, 0b100, 0b010, 0b001, 0b110],
        'T' => [0b111, 0b010, 0b010, 0b010, 0b010],
        'U' => [0b101, 0b101, 0b101, 0b101, 0b111],
        'V' => [0b101, 0b101, 0b101, 0b101, 0b010],
        'W' => [0b101, 0b101, 0b111, 0b111, 0b101],
        'X' => [0b101, 0b101, 0b010, 0b101, 0b101],
        'Y' => [0b101, 0b101, 0b010, 0b010, 0b010],
        'Z' => [0b111, 0b001, 0b010, 0b100, 0b111],
        '0' => [0b111, 0b101, 0b101, 0b101, 0b111],
        '1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        '2' => [0b111, 0b001, 0b111, 0b100, 0b111],
        '3' => [0b111, 0b001, 0b011, 0b001, 0b111],
        '4' => [0b101, 0b101, 0b111, 0b001, 0b001],
        '5' => [0b111, 0b100, 0b111, 0b001, 0b111],
        '6' => [0b111, 0b100, 0b111, 0b101, 0b111],
        '7' => [0b111, 0b001, 0b001, 0b010, 0b010],
        '8' => [0b111, 0b101, 0b111, 0b101, 0b111],
        '9' => [0b111, 0b101, 0b111, 0b001, 0b111],
        ':' => [0b000, 0b010, 0b000, 0b010, 0b000],
        '-' => [0b000, 0b000, 0b111, 0b000, 0b000],
        '.' => [0b000, 0b000, 0b000, 0b000, 0b010],
        _ => [0b000, 0b000, 0b000, 0b000, 0b000],
    }
}
