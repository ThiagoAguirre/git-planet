//! Draws the animation frames.
//!
//! The space scene (star panel, rotating planet, moons) is drawn at half resolution for chunky
//! pixel art and upscaled 2x into the frame; the HUD is then drawn at full resolution on top.

use std::f64::consts::TAU;

use crate::hud;
use crate::palette::Palette;
use crate::planet::{lerp, surface_at, Moon, MoonKind, PlanetBiology};
use crate::snapshot::AccountSnapshot;

pub const FRAME_WIDTH: u32 = 320;
pub const FRAME_HEIGHT: u32 = 140;
pub const FRAMES_PER_SECOND: u32 = 15;
/// One full planet rotation (48 frames = 3.2 seconds), so the GIF loops seamlessly.
pub const LOOP_FRAME_COUNT: u32 = 48;
/// The loop frame used for the still PNG and the share card.
pub const STILL_FRAME: u32 = 6;

pub const OUTLINE_COLOR: [u8; 4] = [12, 8, 16, 255];

const SCENE_SCALE: u32 = 2;
const SCENE_W: u32 = FRAME_WIDTH / SCENE_SCALE;
const SCENE_H: u32 = FRAME_HEIGHT / SCENE_SCALE;

const BAYER_4X4: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];
/// Sunlight from the upper left, toward the viewer (normalized at use).
const LIGHT: [f64; 3] = [-0.62, -0.42, 0.66];
/// Tilt of the spin axis as seen from the camera, in radians.
const AXIAL_TILT: f64 = 0.25;

const SPACE_TOP: [u8; 4] = [5, 6, 18, 255];
const SPACE_BOTTOM: [u8; 4] = [14, 8, 28, 255];
const STAR_BRIGHT: [u8; 4] = [255, 250, 230, 255];
const STAR_MID: [u8; 4] = [190, 180, 210, 255];
const STAR_DIM: [u8; 4] = [120, 118, 150, 255];
const STAR_GLOW: [u8; 4] = [110, 108, 150, 255];
const STAR_COUNT: u32 = 60;
/// Star twinkle period in frames; divides `LOOP_FRAME_COUNT`.
const TWINKLE_PERIOD: u32 = 24;

/// An RGBA frame of `FRAME_WIDTH x FRAME_HEIGHT` pixels.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Frame {
    pub pixels: Vec<[u8; 4]>,
}

impl Frame {
    pub fn as_bytes(&self) -> &[u8] {
        self.pixels.as_flattened()
    }
}

/// Everything that stays constant across the frames of one render.
pub struct Renderer<'a> {
    snapshot: &'a AccountSnapshot,
    palette: &'a Palette,
    bio: PlanetBiology,
    moons: Vec<Moon>,
    seed: u64,
    light: [f64; 3],
}

impl<'a> Renderer<'a> {
    pub fn new(snapshot: &'a AccountSnapshot, palette: &'a Palette) -> Self {
        let bio = PlanetBiology::from_commits(snapshot.total_contributions);
        let len = LIGHT.iter().map(|v| v * v).sum::<f64>().sqrt();
        Self {
            snapshot,
            palette,
            moons: bio.moons(),
            bio,
            seed: snapshot.identity_seed() % 1_000,
            light: LIGHT.map(|v| v / len),
        }
    }

    pub fn biology(&self) -> &PlanetBiology {
        &self.bio
    }

    /// All frames of the seamless loop.
    pub fn render_loop(&self) -> Vec<Frame> {
        (0..LOOP_FRAME_COUNT)
            .map(|i| self.render_frame(i))
            .collect()
    }

    /// Frame `index` of the loop (wraps around).
    pub fn render_frame(&self, index: u32) -> Frame {
        let frame = index % LOOP_FRAME_COUNT;
        let turn = TAU * f64::from(frame) / f64::from(LOOP_FRAME_COUNT);
        let center = (f64::from(SCENE_W) / 2.0, f64::from(SCENE_H) / 2.0 + 1.0);

        let mut scene = Scene::new();
        paint_space(&mut scene, frame);
        for moon in self.moons.iter().filter(|m| !m.in_front(turn)) {
            self.paint_moon(&mut scene, moon, center, turn);
        }
        self.paint_planet(&mut scene, center, turn);
        for moon in self.moons.iter().filter(|m| m.in_front(turn)) {
            self.paint_moon(&mut scene, moon, center, turn);
        }

        let mut pixels = scene.upscaled();
        hud::paint_metrics(&mut pixels, &self.bio, self.palette, self.snapshot);
        Frame { pixels }
    }

    fn paint_moon(&self, scene: &mut Scene, moon: &Moon, center: (f64, f64), turn: f64) {
        let (ox, oy) = moon.offset(turn);
        let (mx, my) = (center.0 + ox, center.1 + oy);
        let ramp = match moon.kind {
            MoonKind::Phobos => &self.palette.phobos,
            MoonKind::Deimos => &self.palette.deimos,
        };
        for y in (my - 3.0) as i64..=(my + 3.0) as i64 {
            for x in (mx - 3.0) as i64..=(mx + 3.0) as i64 {
                let dx = (x as f64 + 0.5 - mx) / (moon.size * 1.3);
                let dy = (y as f64 + 0.5 - my) / moon.size;
                let lump = 0.15 * (dy.atan2(dx) * 3.0 + moon.phase_offset).sin();
                if dx * dx + dy * dy > (1.0 + lump).powi(2) {
                    continue;
                }
                let lit = -dx * 0.7 - dy * 0.5;
                let shade = match lit {
                    l if l > 0.35 => 3,
                    l if l > -0.1 => 2,
                    _ => 1,
                };
                scene.put(x, y, ramp[shade]);
            }
        }
    }

    /// Soil, maria, craters, landmarks, ice caps, limb haze and a dithered day/night line.
    fn paint_planet(&self, scene: &mut Scene, (cx, cy): (f64, f64), turn: f64) {
        let bio = &self.bio;
        let light = self.light;
        let r = bio.radius;
        let (cos_tilt, sin_tilt) = (AXIAL_TILT.cos(), AXIAL_TILT.sin());
        let (cos_rot, sin_rot) = (turn.cos(), turn.sin());
        let haze_edge = 1.0 - 1.6 / r;

        let mut mask = vec![false; scene.px.len()];
        for y in (cy - r) as i64 - 1..=(cy + r) as i64 + 1 {
            for x in (cx - r) as i64 - 1..=(cx + r) as i64 + 1 {
                let Some(i) = Scene::index(x, y) else {
                    continue;
                };
                let nx = (x as f64 + 0.5 - cx) / r;
                let ny = (y as f64 + 0.5 - cy) / r;
                let d = nx * nx + ny * ny;
                if d > 1.0 {
                    continue;
                }
                let nz = (1.0 - d).sqrt();
                let lambert = nx * light[0] + ny * light[1] + nz * light[2];

                // Undo the axial tilt, then spin around the pole.
                let tx = nx * cos_tilt - ny * sin_tilt;
                let ty = nx * sin_tilt + ny * cos_tilt;
                let sx = tx * cos_rot + nz * sin_rot;
                let sz = -tx * sin_rot + nz * cos_rot;
                let surface = surface_at(bio, self.seed, sx, ty, sz);

                let shade = lambert.max(0.0) * 1.15 + 0.08;
                let dither = dither_threshold(x as u32, y as u32) - 0.5;
                let mut color = if surface.ice {
                    let level = (shade * 3.6 + dither * 0.8).floor().clamp(0.0, 3.0);
                    self.palette.ice[level as usize]
                } else {
                    let level = (surface.tone * shade + dither * 0.9)
                        .floor()
                        .clamp(0.0, 5.0);
                    let level = level as usize;
                    blend(
                        self.palette.rock[level],
                        self.palette.soil[level],
                        bio.redness,
                    )
                };

                // Thin dusty haze hugging the sunlit limb, inside the outline.
                let on_limb = d.sqrt() > haze_edge && lambert > 0.05;
                if on_limb && hash_01(x as u32, y as u32) < bio.atmosphere {
                    color = self.palette.haze[usize::from(lambert > 0.45)];
                }

                scene.px[i] = color;
                mask[i] = true;
            }
        }
        scene.outline(&mask);
    }
}

fn hash_01(a: u32, b: u32) -> f64 {
    let mut n = a.wrapping_mul(374_761_393) ^ b.wrapping_mul(668_265_263);
    n = (n ^ (n >> 13)).wrapping_mul(1_274_126_177);
    f64::from((n ^ (n >> 16)) & 0xFFFF) / 65_535.0
}

fn dither_threshold(x: u32, y: u32) -> f64 {
    (f64::from(BAYER_4X4[(y & 3) as usize][(x & 3) as usize]) + 0.5) / 16.0
}

fn blend(a: [u8; 4], b: [u8; 4], t: f64) -> [u8; 4] {
    let mix = |i: usize| lerp(f64::from(a[i]), f64::from(b[i]), t).round() as u8;
    [mix(0), mix(1), mix(2), 255]
}

/// Half-resolution drawing surface.
struct Scene {
    px: Vec<[u8; 4]>,
}

impl Scene {
    fn new() -> Self {
        Self {
            px: vec![[0; 4]; (SCENE_W * SCENE_H) as usize],
        }
    }

    fn index(x: i64, y: i64) -> Option<usize> {
        (x >= 0 && y >= 0 && x < i64::from(SCENE_W) && y < i64::from(SCENE_H))
            .then(|| (y as u32 * SCENE_W + x as u32) as usize)
    }

    fn put(&mut self, x: i64, y: i64, color: [u8; 4]) {
        if let Some(i) = Self::index(x, y) {
            self.px[i] = color;
        }
    }

    /// Paint every pixel outside `mask` that touches a pixel inside it.
    fn outline(&mut self, mask: &[bool]) {
        for y in 0..i64::from(SCENE_H) {
            for x in 0..i64::from(SCENE_W) {
                let Some(i) = Self::index(x, y) else {
                    continue;
                };
                if mask[i] {
                    continue;
                }
                let touches = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                    .iter()
                    .any(|(dx, dy)| Self::index(x + dx, y + dy).is_some_and(|n| mask[n]));
                if touches {
                    self.px[i] = OUTLINE_COLOR;
                }
            }
        }
    }

    fn upscaled(&self) -> Vec<[u8; 4]> {
        (0..FRAME_WIDTH * FRAME_HEIGHT)
            .map(|i| {
                let x = i % FRAME_WIDTH / SCENE_SCALE;
                let y = i / FRAME_WIDTH / SCENE_SCALE;
                self.px[(y * SCENE_W + x) as usize]
            })
            .collect()
    }
}

/// Rounded deep-space panel with a vertical gradient and twinkling stars.
fn paint_space(scene: &mut Scene, frame: u32) {
    let (x0, y0) = (4_i64, 2_i64);
    let (x1, y1) = (i64::from(SCENE_W) - 5, i64::from(SCENE_H) - 3);
    let mut mask = vec![false; scene.px.len()];
    for y in y0..=y1 {
        let color = blend(SPACE_TOP, SPACE_BOTTOM, (y - y0) as f64 / (y1 - y0) as f64);
        for x in x0..=x1 {
            let rounded_corner = (x - x0).min(x1 - x) + (y - y0).min(y1 - y) < 2;
            if let (false, Some(i)) = (rounded_corner, Scene::index(x, y)) {
                scene.px[i] = color;
                mask[i] = true;
            }
        }
    }
    scene.outline(&mask);

    for i in 0..STAR_COUNT {
        let x = x0 + 3 + (hash_01(i, 1) * (x1 - x0 - 6) as f64) as i64;
        let y = y0 + 3 + (hash_01(i, 2) * (y1 - y0 - 6) as f64) as i64;
        let twinkle = (frame + i * 5) % TWINKLE_PERIOD;
        let color = match (twinkle, i % 3) {
            (0 | 1, _) => STAR_BRIGHT,
            (_, 0) => STAR_MID,
            _ => STAR_DIM,
        };
        scene.put(x, y, color);
        if twinkle == 0 && i % 4 == 0 {
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                scene.put(x + dx, y + dy, STAR_GLOW);
            }
        }
    }
}
