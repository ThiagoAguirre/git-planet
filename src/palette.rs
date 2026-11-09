//! Colors for the Mars-like planet. The planet itself keeps Martian colors; the account's top
//! GitHub languages tint the moons (Phobos = #1, Deimos = #2) and are listed in the HUD.

use crate::planet::lerp;
use crate::snapshot::AccountSnapshot;

/// A 4-shade pixel-art ramp `[shadow, mid_dark, mid_light, highlight]` in RGBA.
pub type ShadeRamp = [[u8; 4]; 4];
/// A 6-shade soil ramp from deep shadow to sunlit highlight.
pub type SoilRamp = [[u8; 4]; 6];

/// Rusty Martian soil.
pub const MARS_SOIL: SoilRamp = [
    [46, 16, 14, 255],
    [96, 36, 22, 255],
    [150, 62, 34, 255],
    [196, 96, 52, 255],
    [226, 136, 78, 255],
    [244, 180, 122, 255],
];

/// Gray-brown protoplanet rock (blended into `MARS_SOIL` as the planet matures).
pub const PROTO_ROCK: SoilRamp = [
    [30, 26, 26, 255],
    [58, 50, 46, 255],
    [88, 76, 68, 255],
    [118, 104, 92, 255],
    [148, 132, 116, 255],
    [176, 160, 140, 255],
];

pub const POLAR_ICE: ShadeRamp = [
    [46, 40, 52, 255],
    [120, 112, 124, 255],
    [200, 198, 206, 255],
    [246, 246, 250, 255],
];

/// Dusty limb haze `[dim, bright]`.
pub const LIMB_HAZE: [[u8; 4]; 2] = [[200, 118, 88, 255], [236, 160, 120, 255]];

pub const MOON_ROCK: ShadeRamp = [
    [52, 46, 44, 255],
    [88, 80, 74, 255],
    [128, 118, 108, 255],
    [166, 156, 144, 255],
];

/// How strongly a language color tints its moon.
const MOON_TINT: f64 = 0.22;

/// Complete palette for one render.
#[derive(Clone, Debug)]
pub struct Palette {
    pub soil: SoilRamp,
    pub rock: SoilRamp,
    pub ice: ShadeRamp,
    pub haze: [[u8; 4]; 2],
    pub phobos: ShadeRamp,
    pub deimos: ShadeRamp,
    pub hud_accent: [u8; 4],
    pub hud_text: [u8; 4],
    pub hud_dim: [u8; 4],
}

/// Parse a `#RRGGBB` string into `[r, g, b]`.
pub fn parse_hex_color(hex: &str) -> Option<[u8; 3]> {
    let clean = hex.trim().trim_start_matches('#');
    if clean.len() != 6 || !clean.is_ascii() {
        return None;
    }
    let r = u8::from_str_radix(&clean[0..2], 16).ok()?;
    let g = u8::from_str_radix(&clean[2..4], 16).ok()?;
    let b = u8::from_str_radix(&clean[4..6], 16).ok()?;
    Some([r, g, b])
}

/// Tint every shade of `ramp` toward `rgb` by `weight`.
pub fn tint_ramp(ramp: ShadeRamp, rgb: [u8; 3], weight: f64) -> ShadeRamp {
    ramp.map(|shade| {
        let mix = |i: usize| lerp(f64::from(shade[i]), f64::from(rgb[i]), weight).round() as u8;
        [mix(0), mix(1), mix(2), 255]
    })
}

impl Palette {
    /// Plain Martian palette with untinted gray moons.
    pub fn canon() -> Self {
        Self {
            soil: MARS_SOIL,
            rock: PROTO_ROCK,
            ice: POLAR_ICE,
            haze: LIMB_HAZE,
            phobos: MOON_ROCK,
            deimos: MOON_ROCK,
            hud_accent: MARS_SOIL[4],
            hud_text: [250, 238, 220, 255],
            hud_dim: [210, 196, 186, 255],
        }
    }

    /// Martian palette with moons tinted by the account's top two languages.
    pub fn from_snapshot(snapshot: &AccountSnapshot, use_canon_only: bool) -> Self {
        let mut palette = Self::canon();
        if use_canon_only {
            return palette;
        }
        let lang_rgb = |idx: usize| {
            snapshot
                .languages
                .get(idx)
                .and_then(|l| parse_hex_color(&l.hex_color))
        };
        if let Some(rgb) = lang_rgb(0) {
            palette.phobos = tint_ramp(MOON_ROCK, rgb, MOON_TINT);
        }
        if let Some(rgb) = lang_rgb(1).or_else(|| lang_rgb(0)) {
            palette.deimos = tint_ramp(MOON_ROCK, rgb, MOON_TINT);
        }
        palette
    }
}
